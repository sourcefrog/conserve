// Copyright 2020, 2021, 2022, 2023 Martin Pool.

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

use std::path::Path;
use std::sync::Arc;
use std::{error, fmt, io, result};

use bytes::Bytes;
use derive_more::Display;
use url::Url;

use crate::*;

pub mod local;
use local::LocalTransport;

#[cfg(feature = "s3")]
pub mod s3;

/// Open a `Transport` to access a local directory.
///
/// `s` may be a local path or a URL.
pub fn open_transport(s: &str) -> crate::Result<Arc<dyn Transport>> {
    if let Ok(url) = Url::parse(s) {
        match url.scheme() {
            "file" => Ok(Arc::new(LocalTransport::new(
                &url.to_file_path().expect("extract URL file path"),
            ))),
            #[cfg(feature = "s3")]
            "s3" => Ok(s3::S3Transport::new(&url)?),
            d if d.len() == 1 => {
                // Probably a Windows path with drive letter, like "c:/thing", not actually a URL.
                Ok(Arc::new(LocalTransport::new(Path::new(s))))
            }
            other => Err(crate::Error::UrlScheme {
                scheme: other.to_owned(),
            }),
        }
    } else {
        Ok(Arc::new(LocalTransport::new(Path::new(s))))
    }
}

/// Abstracted filesystem IO to access an archive.
///
/// This supports operations that are common across local filesystems, SFTP, and cloud storage, and
/// that are intended to be sufficient to efficiently implement the Conserve format.
///
/// A transport has a root location, which will typically be the top directory of the Archive.
/// Below that point everything is accessed with a relative path, expressed as a PathBuf.
///
/// All Transports must be `Send + Sync`, so they can be passed across or shared across threads.
///
/// Files in Conserve archives have bounded size and fit in memory so this does not need to
/// support streaming or partial reads and writes.
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// List a directory, separating out file and subdirectory names.
    ///
    /// Names are in the arbitrary order that they're returned from the transport.
    ///
    /// Any error during iteration causes overall failure.
    fn list_dir(&self, relpath: &str) -> Result<ListDir>;

    /// Get one complete file into a caller-provided buffer.
    ///
    /// Files in the archive are of bounded size, so it's OK to always read them entirely into
    /// memory, and this is simple to support on all implementations.
    fn read_file(&self, path: &str) -> Result<Bytes>;

    /// Check if a regular file exists.
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
    ///
    /// If the transport supports it, this should error if the file already exists, returning
    /// [ErrorKind::AlreadyExists]. However, if that can't be done by a single call, it
    /// is OK to simply overwrite the existing object.
    ///
    /// If a temporary file is used, the name should start with `crate::TMP_PREFIX`.
    fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()>;

    /// Get metadata about a file.
    fn metadata(&self, relpath: &str) -> Result<Metadata>;

    /// Delete a file.
    fn remove_file(&self, relpath: &str) -> Result<()>;

    /// Delete a directory and all its contents.
    fn remove_dir_all(&self, relpath: &str) -> Result<()>;

    /// Make a new transport addressing a subdirectory.
    fn sub_transport(&self, relpath: &str) -> Arc<dyn Transport>;

    /// Return a URL scheme describing this transport, such as "file".
    fn url_scheme(&self) -> &'static str;
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

    #[display(fmt = "Other transport error")]
    Other,
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    #[cfg(feature = "s3")]
    pub(self) fn s3_error<K, E, R>(key: K, source: aws_sdk_s3::error::SdkError<E, R>) -> Error
    where
        K: ToOwned<Owned = String>,
        E: std::error::Error + Send + Sync + 'static,
        R: std::fmt::Debug + Send + Sync + 'static,
        ErrorKind: for<'a> From<&'a E>,
    {
        let kind = match &source {
            aws_sdk_s3::error::SdkError::ServiceError(service_err) => {
                ErrorKind::from(service_err.err())
            }
            _ => ErrorKind::Other,
        };
        Error {
            kind,
            path: Some(key.to_owned()),
            source: Some(source.into()),
        }
    }

    pub(self) fn io_error(path: &Path, source: io::Error) -> Error {
        let kind = match source.kind() {
            io::ErrorKind::NotFound => ErrorKind::NotFound,
            io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Other,
        };
        Error {
            source: Some(Box::new(source)),
            path: Some(path.to_string_lossy().to_string()),
            kind,
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
        Ok(())
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        self.source.as_ref().map(|s| &**s as _)
    }
}

type Result<T> = result::Result<T, Error>;
