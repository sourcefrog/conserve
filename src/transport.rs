// Copyright 2020, 2021 Martin Pool.

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

use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::errors::Error;
use crate::kind::Kind;
use crate::Result;

pub mod local;

/// Open a `Transport` to access a local directory.
pub fn open_transport(s: &str) -> Result<Box<dyn Transport>> {
    // TODO: Recognize URL-style strings.
    Ok(Box::new(local::LocalTransport::new(Path::new(s))))
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
    fn read_file(&self, path: &str, out_buf: &mut Vec<u8>) -> io::Result<()>;

    /// Check if an entry exists.
    fn exists(&self, path: &str) -> io::Result<bool>;

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
    fn write_file(&self, relpath: &str, content: &[u8]) -> io::Result<()>;

    /// Get metadata about a file.
    fn metadata(&self, relpath: &str) -> io::Result<Metadata>;

    /// Delete a file.
    fn remove_file(&self, relpath: &str) -> io::Result<()>;

    /// Delete an empty directory.
    fn remove_dir(&self, relpath: &str) -> io::Result<()>;

    /// Delete a directory and all its contents.
    fn remove_dir_all(&self, relpath: &str) -> io::Result<()>;

    /// Make a new transport addressing a subdirectory.
    fn sub_transport(&self, relpath: &str) -> Box<dyn Transport>;

    /// Clone this object into a new box.
    fn box_clone(&self) -> Box<dyn Transport>;
}

impl Clone for Box<dyn Transport> {
    fn clone(&self) -> Box<dyn Transport> {
        self.box_clone()
    }
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
}

/// A list of all the files and directories in a directory.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct ListDirNames {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

/// A path or other URL-like specification of a directory that can be opened as a transport.
///
/// Locations can be parsed from strings. At present the only supported form is an absolute
/// or relative filename.
#[derive(Debug, Eq, PartialEq)]
pub enum Location {
    /// A local directory.
    Local(PathBuf),
}

impl Location {
    /// Open a Transport that can read and write this location.
    ///
    /// The location need not already exist.
    pub fn open(&self) -> Result<Box<dyn Transport>> {
        match self {
            Location::Local(pathbuf) => Ok(Box::new(local::LocalTransport::new(pathbuf))),
        }
    }
}

impl FromStr for Location {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        // Eventually can specifically recognize url or sftp style forms here.
        Ok(Location::Local(s.into()))
    }
}
