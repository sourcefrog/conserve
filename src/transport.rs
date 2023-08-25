// Copyright 2020, 2021, 2022 Martin Pool.

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
use std::{fmt, io};

use anyhow::bail;
use bytes::Bytes;
use url::Url;

use crate::*;

pub mod local;
use local::LocalTransport;

/// Open a `Transport` to access a local directory.
///
/// `s` may be a local path or a URL.
pub fn open_transport(s: &str) -> anyhow::Result<Box<dyn Transport>> {
    if let Ok(url) = Url::parse(s) {
        match url.scheme() {
            "file" => Ok(Box::new(LocalTransport::new(
                &url.to_file_path().expect("extract URL file path"),
            ))),
            d if d.len() == 1 => {
                // Probably a Windows path with drive letter, like "c:/thing", not actually a URL.
                Ok(Box::new(LocalTransport::new(Path::new(s))))
            }
            other => bail!("Unsupported URL scheme {other:?}"),
        }
    } else {
        Ok(Box::new(LocalTransport::new(Path::new(s))))
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
    /// Read the contents of a directory under this transport, without recursing down.
    ///
    /// Returned entries are in arbitrary order and may be interleaved with errors.
    ///
    /// The result should not contain entries for "." and "..".
    fn iter_dir_entries(
        &self,
        path: &str,
    ) -> io::Result<Box<dyn Iterator<Item = io::Result<DirEntry>>>>;

    /// As a convenience, read all filenames from the directory into vecs of
    /// dirs and files.
    ///
    /// Names are in the arbitrary order that they're returned from the transport.
    ///
    /// Any error during iteration causes overall failure.
    fn list_dir_names(&self, relpath: &str) -> io::Result<ListDirNames> {
        let mut names = ListDirNames::default();
        for dir_entry in self.iter_dir_entries(relpath)? {
            let dir_entry = dir_entry?;
            match dir_entry.kind {
                Kind::Dir => names.dirs.push(dir_entry.name),
                Kind::File => names.files.push(dir_entry.name),
                _ => (),
            }
        }
        Ok(names)
    }

    /// Get one complete file into a caller-provided buffer.
    ///
    /// Files in the archive are of bounded size, so it's OK to always read them entirely into
    /// memory, and this is simple to support on all implementations.
    fn read_file(&self, path: &str) -> io::Result<Bytes>;

    /// Check if a directory exists.
    fn is_dir(&self, path: &str) -> anyhow::Result<bool>;

    /// Check if a regular file exists.
    fn is_file(&self, path: &str) -> anyhow::Result<bool>;

    /// Create a directory, if it does not exist.
    ///
    /// If the directory already exists, it's not an error.
    ///
    /// This function does not create missing parent directories.
    fn create_dir(&self, relpath: &str) -> io::Result<()>;

    /// Write a complete file.
    ///
    /// As much as possible, the file should be written atomically so that it is only visible with
    /// the complete content. On a local filesystem the content is written to a temporary file and
    /// then renamed.
    ///
    /// If a temporary file is used, the name should start with `crate::TMP_PREFIX`.
    fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()>;

    /// Get metadata about a file.
    fn metadata(&self, relpath: &str) -> anyhow::Result<Metadata>;

    /// Delete a file.
    fn remove_file(&self, relpath: &str) -> anyhow::Result<()>;

    /// Delete an empty directory.
    fn remove_dir(&self, relpath: &str) -> anyhow::Result<()>;

    /// Delete a directory and all its contents.
    fn remove_dir_all(&self, relpath: &str) -> anyhow::Result<()>;

    /// Make a new transport addressing a subdirectory.
    fn sub_transport(&self, relpath: &str) -> Box<dyn Transport>;

    /// Return a URL scheme describing this transport, such as "file".
    fn url_scheme(&self) -> &'static str;

    /// Return a path or URL for this transport.
    fn url(&self) -> String;
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
pub struct ListDirNames {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

/// A transport error, as a generalization of IO errors.
#[derive(Debug)]
pub struct Error {
    url: Url,
    kind: ErrorKind,
    source: Option<anyhow::Error>,
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub(self) fn io_error(path: &Path, source: io::Error) -> Error {
        let kind = match source.kind() {
            io::ErrorKind::NotFound => ErrorKind::NotFound,
            io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
            _ => ErrorKind::Other,
        };
        Error {
            url: Url::from_file_path(path).expect("Convert path to URL"),
            kind,
            source: Some(source.into()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // source is not in the short format; maybe should be in the alternate format?
        format!("{kind:?}: {url}", kind = self.kind, url = self.url).fmt(f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ErrorKind {
    NotFound,
    AlreadyExists,
    Other,
}

type Result<T> = std::result::Result<T, Error>;
