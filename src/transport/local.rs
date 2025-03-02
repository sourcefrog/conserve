// Copyright 2020-2025 Martin Pool.

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

use std::fs::{create_dir, remove_dir_all, remove_file, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{io, path};

use bytes::Bytes;
use tempfile::TempDir;
use tracing::{error, instrument, trace, warn};
use url::Url;

use super::{Error, ListDir, Metadata, Result, WriteMode};

#[derive(Debug)]
pub(super) struct Protocol {
    path: PathBuf,
    url: Url,
    tempdir: Option<Arc<TempDir>>,
}

impl Protocol {
    pub(super) fn new(path: &Path) -> Self {
        Protocol {
            path: path.to_owned(),
            url: Url::from_directory_path(path::absolute(path).expect("make path absolute"))
                .expect("convert path to URL"),
            tempdir: None,
        }
    }

    /// Create a new temporary directory for testing.
    ///
    /// The tempdir will be removed when all derived Protocols (and Transports)
    /// are dropped.
    ///
    /// # Panics
    ///
    /// If the directory can't be created.
    pub(super) fn temp() -> Self {
        let tempdir = TempDir::new().expect("Create tempdir");
        let path = tempdir.path().to_owned();
        let url = Url::from_directory_path(path::absolute(&path).expect("make path absolute"))
            .expect("convert path to URL");
        Protocol {
            path,
            url,
            tempdir: Some(Arc::new(tempdir)),
        }
    }

    fn full_path(&self, relpath: &str) -> PathBuf {
        debug_assert!(!relpath.contains("/../"), "path must not contain /../");
        self.path.join(relpath)
    }
}

impl super::Protocol for Protocol {
    fn url(&self) -> &Url {
        &self.url
    }

    fn read(&self, relpath: &str) -> Result<Bytes> {
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

    #[instrument(skip(self, content))]
    fn write(&self, relpath: &str, content: &[u8], write_mode: WriteMode) -> Result<()> {
        // TODO: Just write directly; remove if the write fails.
        let full_path = self.full_path(relpath);
        let oops = |err| super::Error::io_error(&full_path, err);
        let mut options = File::options();
        options.write(true);
        match write_mode {
            WriteMode::CreateNew => {
                options.create_new(true);
            }
            WriteMode::Overwrite => {
                options.create(true).truncate(true);
            }
        }
        let mut file = options.open(&full_path).map_err(oops)?;
        if let Err(err) = file.write_all(content) {
            error!("Failed to write {full_path:?}: {err:?}");
            drop(file);
            if let Err(err2) = remove_file(&full_path) {
                error!("Failed to remove {full_path:?}: {err2:?}");
            }
            return Err(oops(err));
        }
        trace!("Wrote {} bytes", content.len());
        Ok(())
    }

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

    fn create_dir(&self, relpath: &str) -> Result<()> {
        let path = self.full_path(relpath);
        create_dir(&path).or_else(|err| {
            if err.kind() == io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(super::Error::io_error(&path, err))
            }
        })
    }

    fn metadata(&self, relpath: &str) -> Result<Metadata> {
        let path = self.full_path(relpath);
        let fsmeta = path.metadata().map_err(|err| Error::io_error(&path, err))?;
        let modified = fsmeta
            .modified()
            .map_err(|err| Error::io_error(&path, err))?
            .into();
        Ok(Metadata {
            len: fsmeta.len(),
            kind: fsmeta.file_type().into(),
            modified,
        })
    }

    fn remove_file(&self, relpath: &str) -> Result<()> {
        let path = self.full_path(relpath);
        remove_file(&path).map_err(|err| super::Error::io_error(&path, err))
    }

    fn remove_dir_all(&self, relpath: &str) -> Result<()> {
        let path = self.full_path(relpath);
        remove_dir_all(&path).map_err(|err| super::Error::io_error(&path, err))
    }

    fn chdir(&self, relpath: &str) -> Arc<dyn super::Protocol> {
        Arc::new(Protocol {
            path: self.path.join(relpath),
            url: self.url.join(relpath).expect("join URL"),
            tempdir: self.tempdir.clone(),
        })
    }

    fn local_path(&self) -> Option<PathBuf> {
        Some(self.path.clone())
    }
}

#[cfg(test)]
mod test {
    use std::error::Error;
    use std::time::Duration;

    use assert_fs::prelude::*;
    use predicates::prelude::*;
    use pretty_assertions::assert_eq;
    use time::OffsetDateTime;

    use super::*;
    use crate::kind::Kind;
    use crate::transport::record::{Call, Verb};
    use crate::transport::{self, Transport};

    #[test]
    fn read_file() {
        let temp = assert_fs::TempDir::new().unwrap();
        let content: &str = "the ribs of the disaster";
        let filename = "poem.txt";

        temp.child(filename).write_str(content).unwrap();

        let transport = Transport::local(temp.path()).enable_record();
        let buf = transport.read(filename).unwrap();
        assert_eq!(buf, content.as_bytes());

        let calls = transport.recorded_calls();
        dbg!(&calls);
        assert_eq!(calls, [Call(Verb::Read, filename.into())]);

        temp.close().unwrap();
    }

    #[test]
    fn read_file_not_found() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path());

        let err = transport
            .read("nonexistent.json")
            .expect_err("read_file should fail on nonexistent file");

        let message = err.to_string();
        assert!(message.contains("Not found"));
        assert!(message.contains("nonexistent.json"));

        assert!(err
            .url
            .as_ref()
            .expect("url")
            .path()
            .ends_with("/nonexistent.json"));
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

        let transport = Transport::local(temp.path());

        let metadata = transport.metadata(filename).unwrap();
        dbg!(&metadata);

        assert_eq!(metadata.len, 24);
        assert_eq!(metadata.kind, Kind::File);
        assert!(metadata.modified + Duration::from_secs(60) > OffsetDateTime::now_utc());
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

        let transport = Transport::local(temp.path());
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
    
    #[cfg(unix)]
    #[test]
    fn list_dir_skips_symlinks() {
        // Archives aren't expected to contain symlinks and so list_dir just skips them.

        let transport = Transport::temp();
        let dir = transport.local_path().unwrap();
        std::os::unix::fs::symlink("foo", dir.join("alink")).unwrap();
        
        let list_dir = transport.list_dir(".").unwrap();
        assert_eq!(list_dir.files, [""; 0]);
        assert_eq!(list_dir.dirs, [""; 0]);
    }

    #[test]
    fn write_file() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path());

        transport.create_dir("subdir").unwrap();
        transport
            .write(
                "subdir/subfile",
                b"Must I paint you a picture?",
                WriteMode::CreateNew,
            )
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
        let transport = Transport::local(temp.path());
        temp.child("file").touch().unwrap();
        fs::set_permissions(temp.child("file").path(), fs::Permissions::from_mode(0o000))
            .expect("set_permissions");

        let err = transport.read("file").unwrap_err();
        assert!(!err.is_not_found());
        assert_eq!(err.kind(), transport::ErrorKind::PermissionDenied);
    }

    #[test]
    fn write_file_can_overwrite() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path());
        let filename = "filename";
        transport
            .write(filename, b"original content", WriteMode::Overwrite)
            .expect("first write succeeds");
        transport
            .write(filename, b"new content", WriteMode::Overwrite)
            .expect("write over existing file succeeds");
        assert_eq!(transport.read(filename).unwrap().as_ref(), b"new content");
    }

    #[test]
    fn create_existing_dir() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path());

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa").unwrap();

        temp.close().unwrap();
    }

    #[test]
    fn sub_transport() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path()).enable_record();

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa/bbb").unwrap();

        let sub_transport = transport.chdir("aaa");
        let sub_list = sub_transport.list_dir("").unwrap();

        assert_eq!(sub_list.dirs, ["bbb"]);
        assert_eq!(sub_list.files, [""; 0]);

        assert_eq!(
            transport.recorded_calls(),
            [
                Call(Verb::CreateDir, "aaa".into()),
                Call(Verb::CreateDir, "aaa/bbb".into()),
                Call(Verb::ListDir, "aaa".into())
            ]
        );

        temp.close().unwrap();
    }

    #[test]
    fn remove_dir_all() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path()).enable_record();

        transport.create_dir("aaa").unwrap();
        transport.create_dir("aaa/bbb").unwrap();
        transport.create_dir("aaa/bbb/ccc").unwrap();

        transport.remove_dir_all("aaa").unwrap();

        assert_eq!(
            *transport.recorded_calls().last().unwrap(),
            Call(Verb::RemoveDirAll, "aaa".into())
        );
    }

    #[test]
    fn temp() {
        let transport = Transport::temp();
        let path = transport.local_path().expect("local_path");
        assert!(path.is_dir());

        // Make some files and directories
        transport
            .write("hey", b"hi there", WriteMode::CreateNew)
            .unwrap();
        transport.create_dir("subdir").unwrap();
        let t2 = transport.chdir("subdir");
        t2.write("subfile", b"subcontent", WriteMode::CreateNew)
            .unwrap();

        // After dropping the first transport, the tempdir still exists
        drop(transport);
        assert!(path.is_dir());
        assert!(t2.list_dir(".").is_ok());

        // After dropping both references, the tempdir is removed
        drop(t2);
        assert!(!path.is_dir());
    }
}
