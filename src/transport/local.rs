// Copyright 2020 Martin Pool.

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

use crate::transport::{DirEntry, Metadata, Transport};

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
        let relpath = relpath.to_owned();
        Ok(Box::new(self.full_path(&relpath).read_dir()?.map(
            move |i| {
                i.and_then(|de| {
                    Ok(DirEntry {
                        name: de.file_name().to_string_lossy().into(),
                        kind: de.file_type()?.into(),
                    })
                })
            },
        )))
    }

    fn read_file(&self, relpath: &str, out_buf: &mut Vec<u8>) -> io::Result<()> {
        out_buf.truncate(0);
        // read_to_end reads in gradually increasing parts, but here we can probably read one large
        // buffer.
        let mut file = File::open(&self.full_path(relpath))?;
        let prefetch_len: usize = file.metadata()?.len().try_into().unwrap();
        out_buf.resize(prefetch_len, 0);
        let actual_len = file.read(out_buf)?;
        out_buf.truncate(actual_len);
        Ok(())
    }

    fn exists(&self, relpath: &str) -> io::Result<bool> {
        Ok(self.full_path(relpath).exists())
    }

    fn box_clone(&self) -> Box<dyn Transport> {
        Box::new(self.clone())
    }

    fn create_dir(&self, relpath: &str) -> io::Result<()> {
        create_dir(self.full_path(&relpath)).or_else(|err| {
            if err.kind() == io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(err)
            }
        })
    }

    fn write_file(&self, relpath: &str, content: &[u8]) -> io::Result<()> {
        let full_path = self.full_path(relpath);
        let dir = full_path.parent().unwrap();
        let mut temp = tempfile::Builder::new()
            .prefix(crate::TMP_PREFIX)
            .tempfile_in(dir)?;
        if let Err(err) = temp.write_all(content) {
            let _ = temp.close();
            return Err(err);
        }
        if let Err(persist_error) = temp.persist(&full_path) {
            let _ = persist_error.file.close()?;
            Err(persist_error.error)
        } else {
            Ok(())
        }
    }

    fn remove_file(&self, relpath: &str) -> io::Result<()> {
        std::fs::remove_file(self.full_path(relpath))
    }

    fn sub_transport(&self, relpath: &str) -> Box<dyn Transport> {
        Box::new(LocalTransport {
            root: self.root.join(relpath),
        })
    }

    fn metadata(&self, relpath: &str) -> io::Result<Metadata> {
        let fsmeta = self.root.join(relpath).metadata()?;
        Ok(Metadata { len: fsmeta.len() })
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
        let mut buf = Vec::new();
        transport.read_file(&filename, &mut buf).unwrap();
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

        assert_eq!(transport.metadata(&filename).unwrap(), Metadata { len: 24 });
        assert!(transport.metadata("nopoem").is_err());
    }

    #[test]
    fn read_with_non_empty_buffer() {
        let mut buf = b"already has some stuff".to_vec();
        let temp = assert_fs::TempDir::new().unwrap();
        let desired = b"content from file";
        let filename = "test.txt";
        temp.child(filename).write_binary(desired).unwrap();
        let transport = LocalTransport::new(temp.path());
        transport.read_file(&filename, &mut buf).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&buf),
            String::from_utf8_lossy(desired)
        );
        temp.close().unwrap();
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

        let transport = Transport::new(&temp.path().to_string_lossy()).unwrap();
        let mut root_list: Vec<_> = transport
            .iter_dir_entries(".")
            .unwrap()
            .map(io::Result::unwrap)
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

        assert_eq!(transport.exists("root file").unwrap(), true);
        assert_eq!(transport.exists("nuh-uh").unwrap(), false);

        let subdir_list: Vec<_> = transport
            .iter_dir_entries("subdir")
            .unwrap()
            .map(io::Result::unwrap)
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
        let transport = Transport::new(&temp.path().to_string_lossy()).unwrap();

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
        let transport = Transport::new(&temp.path().to_string_lossy()).unwrap();

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa").unwrap();

        temp.close().unwrap();
    }

    #[test]
    fn sub_transport() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::new(&temp.path().to_string_lossy()).unwrap();

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa/bbb").unwrap();

        let sub_transport = transport.sub_transport("aaa");
        let sub_list: Vec<DirEntry> = sub_transport
            .iter_dir_entries("")
            .unwrap()
            .map(Result::unwrap)
            .collect();

        assert_eq!(sub_list.len(), 1);
        assert_eq!(sub_list[0].name, "bbb");

        temp.close().unwrap();
    }
}
