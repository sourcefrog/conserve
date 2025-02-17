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

use std::fs::{remove_file, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{io, path};

use async_trait::async_trait;
use bytes::Bytes;
use tempfile::TempDir;
use tokio::sync::Semaphore;
use tracing::{error, instrument, trace, warn};
use url::Url;

use super::{Error, ListDir, Metadata, Result, WriteMode};

/// Avoid opening too many files at once.
static FD_LIMIT: Semaphore = Semaphore::const_new(100);

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

#[async_trait]
impl super::Protocol for Protocol {
    fn url(&self) -> &Url {
        &self.url
    }

    async fn read(&self, relpath: &str) -> Result<Bytes> {
        let full_path = &self.full_path(relpath);
        trace!(?relpath, "Read file");
        tokio::fs::read(full_path)
            .await
            .map(Bytes::from)
            .map_err(|err| Error::io_error(full_path, err))
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

    async fn list_dir_async(&self, relpath: &str) -> Result<ListDir> {
        let _permit = FD_LIMIT.acquire().await.expect("acquire permit");
        let path = self.full_path(relpath);
        trace!("Listing {path:?}");
        let mut listing = ListDir::default();
        let fail = |err| Error::io_error(&path, err);
        let mut read_dir = tokio::fs::read_dir(&path).await.map_err(fail)?;
        while let Some(dir_entry) = read_dir.next_entry().await.map_err(fail)? {
            collect_tokio_dir_entry(&mut listing, dir_entry).await
        }
        Ok(listing)
    }

    async fn create_dir(&self, relpath: &str) -> Result<()> {
        let path = self.full_path(relpath);
        tokio::fs::create_dir(&path).await.or_else(|err| {
            if err.kind() == io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(super::Error::io_error(&path, err))
            }
        })
    }

    async fn metadata(&self, relpath: &str) -> Result<Metadata> {
        let path = self.full_path(relpath);
        let fsmeta = tokio::fs::metadata(&path)
            .await
            .map_err(|err| Error::io_error(&path, err))?;
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

    async fn remove_file(&self, relpath: &str) -> Result<()> {
        let path = self.full_path(relpath);
        tokio::fs::remove_file(&path)
            .await
            .map_err(|err| super::Error::io_error(&path, err))
    }

    async fn remove_dir_all(&self, relpath: &str) -> Result<()> {
        let path = self.full_path(relpath);
        tokio::fs::remove_dir_all(&path)
            .await
            .map_err(|err| super::Error::io_error(&path, err))
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

async fn collect_tokio_dir_entry(list_dir: &mut ListDir, dir_entry: tokio::fs::DirEntry) {
    if let Ok(name) = dir_entry.file_name().into_string() {
        match dir_entry.file_type().await {
            Ok(t) if t.is_dir() => list_dir.dirs.push(name),
            Ok(t) if t.is_file() => list_dir.files.push(name),
            other => warn!("Unexpected file type in archive: {name:?}: {other:?}"),
        }
    } else {
        warn!("Non-UTF-8 filename in archive {:?}", dir_entry.file_name());
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
    use tokio;

    use super::*;
    use crate::kind::Kind;
    use crate::transport::record::{Call, Verb};
    use crate::transport::{self, Transport};

    #[tokio::test]
    async fn read_async() {
        let temp = assert_fs::TempDir::new().unwrap();
        let content: &str = "the ribs of the disaster";
        let filename = "poem.txt";

        temp.child(filename).write_str(content).unwrap();

        let transport = Transport::local(temp.path()).enable_record();

        let bytes = transport.read(filename).await.unwrap();
        assert_eq!(bytes, content.as_bytes());

        let err = transport.read("nonexistent").await.unwrap_err();
        assert!(err.is_not_found());

        let calls = transport.recorded_calls();
        dbg!(&calls);
        assert_eq!(
            calls,
            [
                Call(Verb::Read, filename.into()),
                Call(Verb::Read, "nonexistent".into())
            ]
        );

        temp.close().unwrap();
    }

    #[tokio::test]
    async fn read_file_not_found() {
        let transport = Transport::temp().enable_record();

        let err = transport
            .read("nonexistent.json")
            .await
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

        assert_eq!(
            transport.recorded_calls(),
            [Call(Verb::Read, "nonexistent.json".into())]
        );
    }

    #[tokio::test]
    async fn read_metadata() {
        let temp = assert_fs::TempDir::new().unwrap();
        let content: &str = "the ribs of the disaster";
        let filename = "poem.txt";
        temp.child(filename).write_str(content).unwrap();

        let transport = Transport::local(temp.path()).enable_record();

        let metadata = transport.metadata(filename).await.unwrap();
        dbg!(&metadata);

        assert_eq!(metadata.len, 24);
        assert_eq!(metadata.kind, Kind::File);
        assert!(metadata.modified + Duration::from_secs(60) > OffsetDateTime::now_utc());
        assert!(transport
            .metadata("nopoem")
            .await
            .unwrap_err()
            .is_not_found());
        assert_eq!(
            transport.recorded_calls(),
            [
                Call(Verb::Metadata, filename.into()),
                Call(Verb::Metadata, "nopoem".into())
            ]
        );
    }

    #[tokio::test]
    async fn list_directory() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child("root file").touch().unwrap();
        temp.child("subdir").create_dir_all().unwrap();
        temp.child("subdir")
            .child("subfile")
            .write_str("Morning coffee")
            .unwrap();

        let transport = Transport::local(temp.path());
        let root_list = transport.list_dir(".").await.unwrap();
        assert_eq!(root_list.files, ["root file"]);
        assert_eq!(root_list.dirs, ["subdir"]);

        assert!(transport.is_file("root file").await.unwrap());
        assert!(!transport.is_file("nuh-uh").await.unwrap());

        let subdir_list = transport.list_dir("subdir").await.unwrap();
        assert_eq!(subdir_list.files, ["subfile"]);
        assert_eq!(subdir_list.dirs, [""; 0]);

        temp.close().unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_dir_skips_symlinks() {
        // Archives aren't expected to contain symlinks and so list_dir just skips them.

        let transport = Transport::temp();
        let dir = transport.local_path().unwrap();
        std::os::unix::fs::symlink("foo", dir.join("alink")).unwrap();

        let list_dir = transport.list_dir(".").await.unwrap();
        assert_eq!(list_dir.files, [""; 0]);
        assert_eq!(list_dir.dirs, [""; 0]);
    }

    #[tokio::test]
    async fn write_file() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path());

        transport.create_dir("subdir").await.unwrap();
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
        let dir_meta = transport.metadata("subdir").await.unwrap();
        assert!(dir_meta.kind().is_dir());
        assert!(!dir_meta.kind().is_file());
        assert!(!dir_meta.kind().is_symlink());
        let file_meta = transport.metadata("subdir/subfile").await.unwrap();
        assert!(file_meta.kind().is_file());
        assert!(!file_meta.kind().is_dir());
        assert!(!file_meta.kind().is_symlink());

        temp.close().unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_file_permission_denied() {
        use std::fs;
        use std::os::unix::prelude::PermissionsExt;

        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path());
        temp.child("file").touch().unwrap();
        fs::set_permissions(temp.child("file").path(), fs::Permissions::from_mode(0o000))
            .expect("set_permissions");

        let err = transport.read("file").await.unwrap_err();
        assert!(!err.is_not_found());
        assert_eq!(err.kind(), transport::ErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn write_file_can_overwrite() {
        let transport = Transport::temp().enable_record();
        let filename = "filename";
        transport
            .write(filename, b"original content", WriteMode::Overwrite)
            .expect("first write succeeds");
        transport
            .write(filename, b"new content", WriteMode::Overwrite)
            .expect("write over existing file succeeds");
        assert_eq!(
            transport.read(filename).await.unwrap().as_ref(),
            b"new content"
        );
        assert_eq!(
            transport.recorded_calls(),
            [
                Call(Verb::Write, filename.into()),
                Call(Verb::Write, filename.into()),
                Call(Verb::Read, filename.into())
            ]
        );
    }

    #[tokio::test]
    async fn create_existing_dir() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path());

        transport.create_dir("aaa").await.unwrap();
        transport.create_dir("aaa").await.unwrap();
        transport.create_dir("aaa").await.unwrap();
        assert!(transport.metadata("aaa").await.unwrap().kind().is_dir());

        temp.close().unwrap();
    }

    #[tokio::test]
    async fn sub_transport() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path()).enable_record();

        transport.create_dir("aaa").await.unwrap();
        transport.create_dir("aaa/bbb").await.unwrap();

        let sub_transport = transport.chdir("aaa");
        let sub_list = sub_transport.list_dir("").await.unwrap();

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

    #[tokio::test]
    async fn remove_dir_all() {
        let temp = assert_fs::TempDir::new().unwrap();
        let transport = Transport::local(temp.path()).enable_record();

        transport.create_dir("aaa").await.unwrap();
        transport.create_dir("aaa/bbb").await.unwrap();
        transport.create_dir("aaa/bbb/ccc").await.unwrap();

        transport.remove_dir_all("aaa").await.unwrap();

        assert_eq!(
            *transport.recorded_calls().last().unwrap(),
            Call(Verb::RemoveDirAll, "aaa".into())
        );
    }

    #[tokio::test]
    async fn temp() {
        let transport = Transport::temp();
        let path = transport.local_path().expect("local_path");
        assert!(path.is_dir());

        // Make some files and directories
        transport
            .write("hey", b"hi there", WriteMode::CreateNew)
            .unwrap();
        transport.create_dir("subdir").await.unwrap();
        let t2 = transport.chdir("subdir");
        t2.write("subfile", b"subcontent", WriteMode::CreateNew)
            .unwrap();

        // After dropping the first transport, the tempdir still exists
        drop(transport);
        assert!(path.is_dir());
        assert!(t2.list_dir(".").await.is_ok());

        // After dropping both references, the tempdir is removed
        drop(t2);
        assert!(!path.is_dir());
    }

    #[tokio::test]
    async fn list_dir_async() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child("root file").touch().unwrap();
        temp.child("subdir").create_dir_all().unwrap();
        temp.child("subdir")
            .child("subfile")
            .write_str("Morning coffee")
            .unwrap();

        let transport = Transport::local(temp.path());
        let list_dir = transport.list_dir(".").await.unwrap();
        assert_eq!(list_dir.files, ["root file"]);
        assert_eq!(list_dir.dirs, ["subdir"]);

        let failure = transport.list_dir("nonexistent").await.unwrap_err();
        assert!(failure.is_not_found());
    }
}
