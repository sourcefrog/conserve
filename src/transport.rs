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

//! Filesystem abstraction to read and write local and remote archives.
//!
//! Transport operations return std::io::Result to reflect their narrower focus.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{fmt, result};

use bytes::Bytes;
use time::OffsetDateTime;
use url::Url;

use crate::*;

use self::record::{Call, Verb};

mod error;
pub mod local;
mod protocol;
#[cfg(feature = "sftp")]
pub mod sftp;
use protocol::Protocol;

#[cfg(feature = "s3")]
pub mod s3;

pub mod record;

pub use self::error::{Error, ErrorKind};

/// Abstracted filesystem IO to access an archive.
///
/// This supports operations that are common across local filesystems, SFTP, and cloud storage, and
/// that are intended to be sufficient to efficiently implement the Conserve format.
///
/// A transport has a root location, which will typically be the top directory of the Archive.
/// Below that point everything is accessed with a relative path, expressed as a PathBuf.
///
/// Transport objects can be cheaply cloned.
///
/// Files in Conserve archives have bounded size and fit in memory so this does not need to
/// support streaming or partial reads and writes.
#[derive(Clone)]
pub struct Transport {
    /// The concrete protocol implementation: local, S3, etc.
    protocol: Arc<dyn Protocol + 'static>,

    /// The path relative to the origin.
    ///
    /// This is empty for protocols constructed with `new` etc, and non-empty
    /// for protocols constructed from `chdir`.
    sub_path: String,

    /// If true, record operations into `calls` so that they can be inspected by tests.
    record_calls: bool,

    /// If recording is enabled, a list of all operations on all derived transports.
    calls: Arc<Mutex<Vec<Call>>>,
}

impl Transport {
    /// Open a new local transport addressing a filesystem directory.
    pub fn local(path: &Path) -> Self {
        Transport::from_protocol(Arc::new(local::Protocol::new(path)))
    }

    /// Open a new transport from a string that might be a URL or local path.
    // TODO: Should this be async and open a connection, or should it be lazy?
    // Lazy would be easier for tests that expect local dirs to open immediately.
    // Async might be better for SFTP where we can authenticate the connection...
    pub fn new(s: &str) -> Result<Self> {
        if let Ok(url) = Url::parse(s) {
            Transport::open_url(&url)
        } else {
            Ok(Transport::local(Path::new(s)))
        }
    }

    /// Make a new Transport addressing a new temporary directory.
    ///
    /// This is useful for tests that need a temporary directory.
    ///
    /// The directory will be deleted when all related Transports are dropped.
    ///
    /// # Panics
    ///
    /// If the temporary directory cannot be created.
    pub fn temp() -> Self {
        Transport::from_protocol(Arc::new(local::Protocol::temp()))
    }

    fn from_protocol(protocol: Arc<dyn Protocol>) -> Self {
        Transport {
            protocol,
            record_calls: false,
            sub_path: String::new(),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Open a Transport from a URL.
    // TODO: Like ::new, should this be lazy or prompt?
    pub fn open_url(url: &Url) -> Result<Self> {
        let protocol: Arc<dyn Protocol> = match url.scheme() {
            "file" => Arc::new(local::Protocol::new(
                &url.to_file_path().expect("extract URL file path"),
            )),
            d if d.len() == 1 => {
                // Probably a Windows path with drive letter, like "c:/thing", not actually a URL.
                Arc::new(local::Protocol::new(Path::new(url.as_str())))
            }

            #[cfg(feature = "s3")]
            "s3" => Arc::new(s3::Protocol::new(url)?),

            #[cfg(feature = "sftp")]
            "sftp" => Arc::new(sftp::Protocol::new(url)?),

            _other => {
                return Err(Error {
                    kind: ErrorKind::UrlScheme,
                    url: Some(url.clone()),
                    source: None,
                })
            }
        };
        Ok(Transport::from_protocol(protocol))
    }

    /// Start recording operations from this and any derived transports.
    #[cfg(test)]
    pub(crate) fn enable_record(self) -> Transport {
        Transport {
            record_calls: true,
            ..self
        }
    }

    #[cfg(test)]
    pub(crate) fn recorded_calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// If recording is enabled, record an event.
    fn record(&self, verb: Verb, path: &str) {
        if cfg!(test) && self.record_calls {
            let mut full_path = self.sub_path.clone();
            if !path.is_empty() {
                if !full_path.is_empty() {
                    full_path += "/";
                }
                full_path += path;
            }
            self.calls.lock().unwrap().push(Call(verb, full_path));
        }
    }

    /// Get one complete file.
    ///
    /// Files in the archive are of bounded size, so it's OK to always read them
    /// entirely into memory, and this is simple to support on all
    /// implementations.
    pub async fn read(&self, path: &str) -> Result<Bytes> {
        self.record(Verb::Read, path);
        self.protocol.read(path).await
    }

    pub async fn list_dir(&self, relpath: &str) -> Result<ListDir> {
        // TODO: Perhaps it'd be better to include sizes (and maybe mtimes) as many transports
        // might be able to provide this without extra work.
        self.record(Verb::ListDir, relpath);
        self.protocol.list_dir_async(relpath).await
    }

    /// Make a new transport addressing a subdirectory.
    ///
    /// This can succeed even if the subdirectory does not exist yet.
    pub fn chdir(&self, relpath: &str) -> Self {
        let mut sub_path = self.sub_path.clone();
        if !relpath.is_empty() {
            if !sub_path.is_empty() {
                sub_path += "/";
            }
            sub_path += relpath;
        }
        Transport {
            protocol: self.protocol.chdir(relpath),
            sub_path,
            record_calls: self.record_calls,
            calls: Arc::clone(&self.calls),
        }
    }

    pub async fn write(&self, relpath: &str, content: &[u8], mode: WriteMode) -> Result<()> {
        self.record(Verb::Write, relpath);
        self.protocol.write(relpath, content, mode).await
    }

    pub async fn create_dir(&self, relpath: &str) -> Result<()> {
        self.record(Verb::CreateDir, relpath);
        self.protocol.create_dir(relpath).await
    }

    /// Return mtime, size, and other metadata about a file.
    pub async fn metadata(&self, relpath: &str) -> Result<Metadata> {
        self.record(Verb::Metadata, relpath);
        self.protocol.metadata(relpath).await
    }

    /// Delete a file.
    pub async fn remove_file(&self, relpath: &str) -> Result<()> {
        self.record(Verb::RemoveFile, relpath);
        self.protocol.remove_file(relpath).await
    }

    /// Delete a directory and all its contents.
    pub async fn remove_dir_all(&self, relpath: &str) -> Result<()> {
        self.record(Verb::RemoveDirAll, relpath);
        self.protocol.remove_dir_all(relpath).await
    }

    /// Check if a regular file exists.
    pub async fn is_file(&self, path: &str) -> Result<bool> {
        match self.metadata(path).await {
            Ok(metadata) => Ok(metadata.kind == Kind::File),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn url(&self) -> &Url {
        self.protocol.url()
    }

    /// If this is a local transport, return the path.
    pub fn local_path(&self) -> Option<PathBuf> {
        self.protocol.local_path()
    }
}

impl fmt::Debug for Transport {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Transport({})", self.url())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Create the file if it does not exist, or overwrite it if it does.
    Overwrite,

    /// Create the file if it does not exist, or fail if it does.
    CreateNew,
}

/// A directory entry read from a transport.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DirEntry {
    /// Name of the file within the directory being listed.
    pub name: String,
    /// Kind of file.
    pub kind: Kind,
}

/// Stat metadata about a file in a transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Metadata {
    /// File length.
    pub len: u64,

    /// Kind of file.
    pub kind: Kind,

    /// Last modified time.
    pub modified: OffsetDateTime,
}

impl Metadata {
    pub fn kind(&self) -> Kind {
        self.kind
    }
}

/// A list of all the files and directories in a directory.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct ListDir {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

type Result<T> = result::Result<T, Error>;

#[cfg(test)]
mod test {
    use std::path::Path;

    use assert_fs::{prelude::*, TempDir};
    use pretty_assertions::assert_eq;

    use super::Transport;

    #[test]
    fn get_path_from_local_transport() {
        let transport = Transport::local(Path::new("/tmp"));
        assert_eq!(transport.local_path().as_deref(), Some(Path::new("/tmp")));
    }

    #[test]
    fn local_transport_debug_form() {
        let transport = Transport::local(Path::new("/tmp"));
        #[cfg(unix)]
        assert_eq!(format!("{:?}", transport), "Transport(file:///tmp/)");
        #[cfg(windows)]
        {
            use regex::Regex;
            let dbg = format!("{:?}", transport);
            dbg!(&dbg);
            let re = Regex::new(r#"Transport\(file:///[A-Za-z]:/tmp/\)"#).unwrap();
            assert!(re.is_match(&dbg));
        }
    }

    #[tokio::test]
    async fn local_list_dir_async() {
        let temp = TempDir::new().unwrap();
        let transport = Transport::local(temp.path());
        temp.child("a").touch().unwrap();
        let list = transport.list_dir(".").await.unwrap();
        assert_eq!(list.files, ["a"]);
        assert!(list.dirs.is_empty());
    }
}
