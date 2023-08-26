// Copyright 2020-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Access to an archive on the local filesystem.

use std::convert::TryInto;
use std::fs::{create_dir, File};
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use metrics::{counter, increment_counter};

use super::{DirEntry, Error, Metadata, Result, Transport};

#[derive(Clone, Debug)]
pub struct LocalTransport {
    /// Root directory for this transport.
    root: PathBuf,
}

impl LocalTransport {
    pub fn new(path: &Path) -> Self {
        LocalTransport {
            root: path.to_owned(),
        }
    }

    pub fn full_path(&self, relpath: &str) -> PathBuf {
        debug_assert!(!relpath.contains("/../"), "path must not contain /../");
        self.root.join(relpath)
    }
}

impl Transport for LocalTransport {
    fn iter_dir_entries(
        &self,
        relpath: &str,
    ) -> io::Result<Box<dyn Iterator<Item = io::Result<DirEntry>>>> {
        // Archives should never normally contain non-UTF-8 (or even non-ASCII) filenames, but
        // let's pass them back as lossy UTF-8 so they can be reported at a higher level, for
        // example during validation.
        let full_path = self.full_path(relpath);
        increment_counter!("conserve.local_transport.read_dirs");
        Ok(Box::new(full_path.read_dir()?.map(move |de_result| {
            let de = de_result?;
            Ok(DirEntry {
                name: de.file_name().to_string_lossy().into(),
                kind: de.file_type()?.into(),
            })
        })))
    }

    fn read_file(&self, relpath: &str) -> Result<Bytes> {
        increment_counter!("conserve.local_transport.read_files");
        fn try_block(path: &Path) -> io::Result<Bytes> {
            let mut file = File::open(path)?;
            let estimated_len: usize = file
                .metadata()?
                .len()
                .try_into()
                .expect("File size fits in usize");
            let mut out_buf = Vec::with_capacity(estimated_len);
            let actual_len = file.read_to_end(&mut out_buf)?;
            counter!(
                "conserve.local_transport.read_file_bytes",
                actual_len as u64
            );
            out_buf.truncate(actual_len);
            Ok(out_buf.into())
        }
        let path = &self.full_path(relpath);
        try_block(path).map_err(|err| Error::io_error(path, err))
    }

    fn is_file(&self, relpath: &str) -> Result<bool> {
        increment_counter!("conserve.local_transport.metadata_reads");
        let path = self.full_path(relpath);
        Ok(path.is_file())
    }

    fn is_dir(&self, relpath: &str) -> Result<bool> {
        increment_counter!("conserve.local_transport.metadata_reads");
        let path = self.full_path(relpath);
        Ok(path.is_dir())
    }

    fn create_dir(&self, relpath: &str) -> super::Result<()> {
        let path = self.full_path(relpath);
        create_dir(&path).or_else(|err| {
            if err.kind() == io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(super::Error::io_error(&path, err))
            }
        })
    }

    fn write_file(&self, relpath: &str, content: &[u8]) -> super::Result<()> {
        increment_counter!("conserve.local_transport.write_files");
        counter!(
            "conserve.local_transport.write_file_bytes",
            content.len() as u64
        );
        let full_path = self.full_path(relpath);
        let dir = full_path.parent().unwrap();
        let context = |err| super::Error::io_error(&full_path, err);
        let mut temp = tempfile::Builder::new()
            .prefix(crate::TMP_PREFIX)
            .tempfile_in(dir)
            .map_err(context)?;
        if let Err(err) = temp.write_all(content) {
            let _ = temp.close();
            return Err(context(err));
        }
        if let Err(persist_error) = temp.persist(&full_path) {
            persist_error.file.close().map_err(context)?;
            Err(context(persist_error.error))
        } else {
            Ok(())
        }
    }

    fn remove_file(&self, relpath: &str) -> super::Result<()> {
        let path = self.full_path(relpath);
        std::fs::remove_file(&path).map_err(|err| super::Error::io_error(&path, err))
    }

    fn remove_dir(&self, relpath: &str) -> super::Result<()> {
        let path = self.full_path(relpath);
        std::fs::remove_dir(&path).map_err(|err| super::Error::io_error(&path, err))
    }

    fn remove_dir_all(&self, relpath: &str) -> super::Result<()> {
        let path = self.full_path(relpath);
        std::fs::remove_dir_all(&path).map_err(|err| super::Error::io_error(&path, err))
    }

    fn sub_transport(&self, relpath: &str) -> Box<dyn Transport> {
        Box::new(LocalTransport {
            root: self.root.join(relpath),
        })
    }

    fn metadata(&self, relpath: &str) -> Result<Metadata> {
        increment_counter!("conserve.local_transport.metadata_reads");
        let path = self.root.join(relpath);
        let fsmeta = path.metadata().map_err(|err| Error::io_error(&path, err))?;
        Ok(Metadata {
            len: fsmeta.len(),
            kind: fsmeta.file_type().into(),
        })
    }

    fn url_scheme(&self) -> &'static str {
        "file"
    }

    fn url(&self) -> String {
        // TODO: An actual URL.
        self.root.to_string_lossy().into()
    }
}

impl AsRef<dyn Transport> for LocalTransport {
    fn as_ref(&self) -> &(dyn Transport + 'static) {
        self
    }
}

#[cfg(test)]
mod test {
    use assert_fs::prelude::*;
    use predicates::prelude::*;

    use super::*;
    use crate::kind::Kind;

    #[test]
    fn read_file() {
        let temp = assert_fs::TempDir::new().unwrap();
        let content: &str = "the ribs of the disaster";
        let filename = "poem.txt";

        temp.child(filename).write_str(content).unwrap();

        let transport = LocalTransport::new(temp.path());
        let buf = transport.read_file(filename).unwrap();
        assert_eq!(buf, content.as_bytes());

        temp.close().unwrap();
    }

    #[test]
    fn read_metadata() {
        let temp = assert_fs::TempDir::new().unwrap();
        let content: &str = "the ribs of the disaster";
        let filename = "poem.txt";
        temp.child(filename).write_str(content).unwrap();

        let transport = LocalTransport::new(temp.path());

        assert_eq!(
            transport.metadata(filename).unwrap(),
            Metadata {
                len: 24,
                kind: Kind::File
            }
        );
        assert!(transport.metadata("nopoem").is_err());
    }

    #[test]
    fn list_directory() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child("root file").touch().unwrap();
        temp.child("subdir").create_dir_all().unwrap();
        temp.child("subdir")
            .child("subfile")
            .write_str("Morning coffee")
            .unwrap();

        let transport = LocalTransport::new(temp.path());
        let mut root_list: Vec<_> = transport
            .iter_dir_entries(".")
            .unwrap()
            .map(std::io::Result::unwrap)
            .collect();
        assert_eq!(root_list.len(), 2);
        root_list.sort();

        assert_eq!(
            root_list[0],
            DirEntry {
                name: "root file".to_owned(),
                kind: Kind::File,
            }
        );

        // Len is unpredictable for directories, so check the other fields.
        assert_eq!(root_list[1].name, "subdir");
        assert_eq!(root_list[1].kind, Kind::Dir);

        assert!(transport.is_file("root file").unwrap());
        assert!(!transport.is_file("nuh-uh").unwrap());

        let subdir_list: Vec<_> = transport
            .iter_dir_entries("subdir")
            .unwrap()
            .map(std::io::Result::unwrap)
            .collect();
        assert_eq!(
            subdir_list,
            vec![DirEntry {
                name: "subfile".to_owned(),
                kind: Kind::File,
            }]
        );

        temp.close().unwrap();
    }

    #[test]
    fn write_file() {
        // TODO: Maybe test some error cases of failing to write.
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = LocalTransport::new(temp.path());

        transport.create_dir("subdir").unwrap();
        transport
            .write_file("subdir/subfile", b"Must I paint you a picture?")
            .unwrap();

        temp.child("subdir").assert(predicate::path::is_dir());
        temp.child("subdir")
            .child("subfile")
            .assert("Must I paint you a picture?");

        temp.close().unwrap();
    }

    #[test]
    fn create_existing_dir() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = LocalTransport::new(temp.path());

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa").unwrap();

        temp.close().unwrap();
    }

    #[test]
    fn sub_transport() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = LocalTransport::new(temp.path());

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa/bbb").unwrap();

        let sub_transport = transport.sub_transport("aaa");
        let sub_list: Vec<DirEntry> = sub_transport
            .iter_dir_entries("")
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(sub_list.len(), 1);
        assert_eq!(sub_list[0].name, "bbb");

        temp.close().unwrap();
    }

    #[test]
    fn remove_dir_all() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = LocalTransport::new(temp.path());

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa/bbb").unwrap();
        transport.create_dir("aaa/bbb/ccc").unwrap();

        transport.remove_dir_all("aaa").unwrap();
    }
}
