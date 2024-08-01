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

use std::fs::{create_dir, File};
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use tracing::{instrument, trace, warn};

use super::{Error, ListDir, Metadata, Result, Transport};

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
    fn list_dir(&self, relpath: &str) -> Result<ListDir> {
        // Archives should never normally contain non-UTF-8 (or even non-ASCII) filenames, but
        // let's pass them back as lossy UTF-8 so they can be reported at a higher level, for
        // example during validation.
        let path = self.full_path(relpath);
        let fail = |err| Error::io_error(&path, err);
        let mut names = ListDir::default();
        for dir_entry in path.read_dir().map_err(fail)? {
            let dir_entry = dir_entry.map_err(fail)?;
            if let Ok(name) = dir_entry.file_name().into_string() {
                match dir_entry.file_type().map_err(fail)? {
                    t if t.is_dir() => names.dirs.push(name),
                    t if t.is_file() => names.files.push(name),
                    _ => (),
                }
            } else {
                // These should never normally exist in archive directories, so warn
                // and continue.
                warn!("Non-UTF-8 filename in archive {:?}", dir_entry.file_name());
            }
        }
        Ok(names)
    }

    #[instrument(skip(self))]
    fn read_file(&self, relpath: &str) -> Result<Bytes> {
        fn try_block(path: &Path) -> io::Result<Bytes> {
            let mut file = File::open(path)?;
            let estimated_len: usize = file
                .metadata()?
                .len()
                .try_into()
                .expect("File size fits in usize");
            let mut out_buf = Vec::with_capacity(estimated_len);
            let actual_len = file.read_to_end(&mut out_buf)?;
            trace!("Read {actual_len} bytes");
            out_buf.truncate(actual_len);
            Ok(out_buf.into())
        }
        let path = &self.full_path(relpath);
        try_block(path).map_err(|err| Error::io_error(path, err))
    }

    fn is_file(&self, relpath: &str) -> Result<bool> {
        let path = self.full_path(relpath);
        Ok(path.is_file())
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

    #[instrument(skip(self, content))]
    fn write_file(&self, relpath: &str, content: &[u8]) -> super::Result<()> {
        let full_path = self.full_path(relpath);
        let dir = full_path.parent().unwrap();
        let context = |err| super::Error::io_error(&full_path, err);
        let mut temp = tempfile::Builder::new()
            .prefix(crate::TMP_PREFIX)
            .tempfile_in(dir)
            .map_err(context)?;
        if let Err(err) = temp.write_all(content) {
            let _ = temp.close();
            warn!("Failed to write {:?}: {:?}", relpath, err);
            return Err(context(err));
        }
        if let Err(persist_error) = temp.persist(&full_path) {
            warn!("Failed to persist {:?}: {:?}", full_path, persist_error);
            persist_error.file.close().map_err(context)?;
            Err(context(persist_error.error))
        } else {
            trace!("Wrote {} bytes", content.len());
            Ok(())
        }
    }

    fn remove_file(&self, relpath: &str) -> super::Result<()> {
        let path = self.full_path(relpath);
        std::fs::remove_file(&path).map_err(|err| super::Error::io_error(&path, err))
    }

    fn remove_dir_all(&self, relpath: &str) -> super::Result<()> {
        let path = self.full_path(relpath);
        std::fs::remove_dir_all(&path).map_err(|err| super::Error::io_error(&path, err))
    }

    fn sub_transport(&self, relpath: &str) -> Arc<dyn Transport> {
        Arc::new(LocalTransport {
            root: self.root.join(relpath),
        })
    }

    fn metadata(&self, relpath: &str) -> Result<Metadata> {
        let path = self.root.join(relpath);
        let fsmeta = path.metadata().map_err(|err| Error::io_error(&path, err))?;
        Ok(Metadata {
            len: fsmeta.len(),
            kind: fsmeta.file_type().into(),
        })
    }
}

impl AsRef<dyn Transport> for LocalTransport {
    fn as_ref(&self) -> &(dyn Transport + 'static) {
        self
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;

    use assert_fs::prelude::*;
    use predicates::prelude::*;

    use super::*;
    use crate::kind::Kind;
    use crate::transport;

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
    fn read_file_not_found() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = LocalTransport::new(temp.path());

        let err = transport
            .read_file("nonexistent.json")
            .expect_err("read_file should fail on nonexistent file");

        let message = err.to_string();
        assert!(message.contains("Not found"));
        assert!(message.contains("nonexistent.json"));

        assert!(err.path().expect("path").ends_with("nonexistent.json"));
        assert_eq!(err.kind(), transport::ErrorKind::NotFound);
        assert!(err.is_not_found());

        let source = err.source().expect("source");
        let io_source: &io::Error = source.downcast_ref().expect("io::Error");
        assert_eq!(io_source.kind(), io::ErrorKind::NotFound);
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
        assert!(transport.metadata("nopoem").unwrap_err().is_not_found());
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
        let root_list = transport.list_dir(".").unwrap();
        assert_eq!(root_list.files, ["root file"]);
        assert_eq!(root_list.dirs, ["subdir"]);

        assert!(transport.is_file("root file").unwrap());
        assert!(!transport.is_file("nuh-uh").unwrap());

        let subdir_list = transport.list_dir("subdir").unwrap();
        assert_eq!(subdir_list.files, ["subfile"]);
        assert_eq!(subdir_list.dirs, [""; 0]);

        temp.close().unwrap();
    }

    #[test]
    fn write_file() {
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

    #[cfg(unix)]
    #[test]
    fn write_file_permission_denied() {
        use std::fs;
        use std::os::unix::prelude::PermissionsExt;

        let temp = assert_fs::TempDir::new().unwrap();
        let transport = LocalTransport::new(temp.path());
        temp.child("file").touch().unwrap();
        fs::set_permissions(temp.child("file").path(), fs::Permissions::from_mode(0o000))
            .expect("set_permissions");

        let err = transport.read_file("file").unwrap_err();
        assert!(!err.is_not_found());
        assert_eq!(err.kind(), transport::ErrorKind::PermissionDenied);
    }

    #[test]
    fn write_file_can_overwrite() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = LocalTransport::new(temp.path());
        let filename = "filename";
        transport
            .write_file(filename, b"original content")
            .expect("first write succeeds");
        transport
            .write_file(filename, b"new content")
            .expect("write over existing file succeeds");
        assert_eq!(
            transport.read_file(filename).unwrap().as_ref(),
            b"new content"
        );
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
        let sub_list = sub_transport.list_dir("").unwrap();

        assert_eq!(sub_list.dirs, ["bbb"]);
        assert_eq!(sub_list.files, [""; 0]);

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
