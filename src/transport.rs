// Copyright 2020-2024 Martin Pool.

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
use std::sync::Arc;
use std::{error, fmt, io, result};

use bytes::Bytes;
use derive_more::Display;
use url::Url;

use crate::*;

pub mod local;
#[cfg(feature = "sftp")]
pub mod sftp;

#[cfg(feature = "s3")]
pub mod s3;

/// Open a `Transport` to access a local directory.
///
/// `s` may be a local path or a URL.
// TODO: Deprecate for Transport::new
pub fn open_transport(s: &str) -> crate::Result<Transport2> {
    Ok(Transport2::new(s)?)
}

// TODO: Deprecate and use Transport2::local
pub fn open_local_transport(path: &Path) -> Result<Transport2> {
    Ok(Transport2::local(path))
}

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
pub struct Transport2 {
    protocol: Arc<dyn Protocol+ 'static>,
}

impl Transport2 {
    /// Open a new local transport addressing a filesystem directory.
    pub fn local(path: &Path) -> Self {
        Transport2 {
            protocol: Arc::new(local::Protocol::new(path)),
        }
    }

    /// Open a new transport from a string that might be a URL or local path.
    pub fn new(s: &str) -> Result<Self> {
        if let Ok(url) = Url::parse(s) {
            Transport2::from_url(&url)
        } else {
            Ok(Transport2::local(Path::new(s)))
        }
    }

    pub fn from_url(url: &Url) -> Result<Self> {
        let protocol: Arc<dyn Protocol> = match url.scheme() {
            "file" => Arc::new(local::Protocol::new(
                &url.to_file_path().expect("extract URL file path"),
            )),
            d if d.len() == 1 => {
                // Probably a Windows path with drive letter, like "c:/thing", not actually a URL.
                Arc::new(local::Protocol::new(Path::new(url.as_str())))
            }

            #[cfg(feature = "s3")]
            "s3" => Arc::new(s3::Protocol::new(&url)?),

            #[cfg(feature = "sftp")]
            "sftp" => Arc::new(sftp::Protocol::new(&url)?),

            _other => {
                return Err(Error {
                    kind: ErrorKind::UrlScheme,
                    path: Some(url.as_str().to_owned()),
                    source: None,
                })
            }
        };
        Ok(Transport2 {
            protocol,
        })
    }

    /// Get one complete file into a caller-provided buffer.
    ///
    /// Files in the archive are of bounded size, so it's OK to always read them entirely into
    /// memory, and this is simple to support on all implementations.
    pub fn read_file(&self, path: &str) -> Result<Bytes> {
        self.protocol.read_file(path)
    }

    /// List a directory, separating out file and subdirectory names.
    ///
    /// Names are in the arbitrary order that they're returned from the transport.
    ///
    /// Any error during iteration causes overall failure.
    pub fn list_dir(&self, relpath: &str) -> Result<ListDir> {
        self.protocol.list_dir(relpath)
    }

    /// Make a new transport addressing a subdirectory.
    pub fn sub_transport(&self, relpath: &str) -> Self {
        Transport2 {
            protocol: self.protocol.chdir(relpath),
        }
    }

    pub fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()> {
        self.protocol.write_file(relpath, content)
    }

    pub fn create_dir(&self, relpath: &str) -> Result<()> {
        self.protocol.create_dir(relpath)
    }

    pub fn metadata(&self, relpath: &str) -> Result<Metadata> {
        self.protocol.metadata(relpath)
    }

    /// Delete a file.
    pub fn remove_file(&self, relpath: &str) -> Result<()> {
        self.protocol.remove_file(relpath)
    }

    /// Delete a directory and all its contents.
    pub fn remove_dir_all(&self, relpath: &str) -> Result<()> {
        self.protocol.remove_dir_all(relpath)
    }

    /// Check if a regular file exists.
    pub fn is_file(&self, path: &str) -> Result<bool> {
        match self.metadata(path) {
            Ok(metadata) => Ok(metadata.kind == Kind::File),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub fn url(&self) -> &Url {
        self.protocol.url()
    }

    #[allow(unused)]
    fn local_path(&self) -> Option<PathBuf> {
        self.protocol.local_path()
    }
}

impl fmt::Debug for Transport2 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Transport2({})", self.url())
    }
}

trait Protocol: Send + Sync {
    fn read_file(&self, path: &str) -> Result<Bytes>;
    fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()>;
    fn list_dir(&self, relpath: &str) -> Result<ListDir>;
    fn create_dir(&self, relpath: &str) -> Result<()>;
    fn metadata(&self, relpath: &str) -> Result<Metadata>;

    /// Delete a file.
    fn remove_file(&self, relpath: &str) -> Result<()>;

    /// Delete a directory and all its contents.
    fn remove_dir_all(&self, relpath: &str) -> Result<()>;

    /// Make a new transport addressing a subdirectory.
    fn chdir(&self, relpath: &str) -> Arc<dyn Protocol>;

    fn url(&self) -> &Url;

    fn local_path(&self) -> Option<PathBuf> {
        None
    }
}

pub trait Transport: Send + Sync + std::fmt::Debug {
    fn list_dir(&self, relpath: &str) -> Result<ListDir>;

    fn read_file(&self, path: &str) -> Result<Bytes>;

    fn is_file(&self, path: &str) -> Result<bool> {
        match self.metadata(path) {
            Ok(metadata) => Ok(metadata.kind == Kind::File),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Create a directory, if it does not exist.
    ///
    /// If the directory already exists, it's not an error.
    ///
    /// This function does not create missing parent directories.
    fn create_dir(&self, relpath: &str) -> Result<()>;

    /// Write a complete file.
    ///
    /// As much as possible, the file should be written atomically so that it is only visible with
    /// the complete content. On a local filesystem the content is written to a temporary file and
    /// then renamed.
    /// If a temporary file is used, the name should start with `crate::TMP_PREFIX`.
    ///
    /// If the file exists it is replaced. (Across transports, and particularly on S3,
    /// we can't rely on detecting existing files.)
    fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()>;

    /// Get metadata about a file.
    fn metadata(&self, relpath: &str) -> Result<Metadata>;

    /// Delete a file.
    fn remove_file(&self, relpath: &str) -> Result<()>;

    /// Delete a directory and all its contents.
    fn remove_dir_all(&self, relpath: &str) -> Result<()>;

    /// Make a new transport addressing a subdirectory.
    fn sub_transport(&self, relpath: &str) -> Arc<dyn Transport>;
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
}

/// A list of all the files and directories in a directory.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct ListDir {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

/// A transport error, as a generalization of IO errors.
#[derive(Debug)]
pub struct Error {
    /// What type of generally known error?
    kind: ErrorKind,
    /// The underlying error: for example an IO or S3 error.
    source: Option<Box<dyn error::Error + Send + Sync>>,
    /// The affected path, possibly relative to the transport.
    path: Option<String>,
}

/// General categories of transport errors.
#[derive(Debug, Display, PartialEq, Eq, Clone, Copy)]
pub enum ErrorKind {
    #[display(fmt = "Not found")]
    NotFound,

    #[display(fmt = "Already exists")]
    AlreadyExists,

    #[display(fmt = "Permission denied")]
    PermissionDenied,

    #[display(fmt = "Unsupported URL scheme")]
    UrlScheme,

    #[display(fmt = "Other transport error")]
    Other,
}

impl From<io::ErrorKind> for ErrorKind {
    fn from(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => ErrorKind::NotFound,
            io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Other,
        }
    }
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub(self) fn io_error(path: &Path, source: io::Error) -> Error {
        Error {
            kind: source.kind().into(),
            source: Some(Box::new(source)),
            path: Some(path.to_string_lossy().to_string()),
        }
    }

    pub fn is_not_found(&self) -> bool {
        self.kind == ErrorKind::NotFound
    }

    /// The transport-relative path where this error occurred, if known.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(ref path) = self.path {
            write!(f, ": {}", path)?;
        }
        if let Some(source) = &self.source {
            // I'm not sure we should write this here; it might be repetitive.
            write!(f, ": {source}")?;
        }
        Ok(())
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        self.source.as_ref().map(|s| &**s as _)
    }
}

type Result<T> = result::Result<T, Error>;

#[cfg(test)]
mod test {
    use std::path::Path;

    use super::Transport2;

    #[test]
    fn get_path_from_local_transport() {
        let transport = Transport2::local(Path::new("/tmp"));
        assert_eq!(transport.local_path().as_deref(), Some(Path::new("/tmp")));
    }
}
