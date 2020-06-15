// Copyright 2020 Martin Pool.

//! Filesystem abstraction to read and write local and remote archives.
//!
//! Transport operations return std::io::Result to reflect their narrower focus.

use std::io;
use std::path::{Path, PathBuf};

use crate::kind::Kind;
use crate::Result;

pub mod local;

/// Abstracted filesystem IO ta access an archive.
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

impl dyn Transport {
    pub fn new(s: &str) -> Result<Box<dyn Transport>> {
        // TODO: Recognize URL-style strings.
        Ok(Box::new(local::LocalTransport::new(&Path::new(s))))
    }
}

/// A directory entry read from a transport.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DirEntry {
    /// Name of the file within the directory being listed.
    pub name: String,
    pub kind: Kind,
    /// Size in bytes.
    pub len: u64,
}

/// A list of all the files and directories in a directory.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct ListDirNames {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
}

/// A path or other URL-like specification of a directory that can be opened as a transport.
#[derive(Debug, Eq, PartialEq)]
pub enum Location {
    /// A local directory.
    Local(PathBuf),
}

impl Location {
    /// Open a Transport that can read and write this location.
    ///
    /// The location need not already exist.
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use conserve::transport::Location;
    ///
    /// let location = Location::Local("/backup".to_owned().into());
    /// let transport = location.open().unwrap();
    /// ```
    pub fn open(&self) -> Result<Box<dyn Transport>> {
        match self {
            Location::Local(pathbuf) => Ok(Box::new(local::LocalTransport::new(&pathbuf))),
        }
    }
}

#[cfg(test)]
mod test {
    use assert_fs::prelude::*;

    use super::*;
    use crate::transport::local::LocalTransport;

    #[test]
    fn list_dir_names() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child("a dir").create_dir_all().unwrap();
        temp.child("a file").touch().unwrap();
        temp.child("another file").touch().unwrap();

        let transport = LocalTransport::new(&temp.path());

        let ListDirNames { mut files, dirs } = transport.list_dir_names("").unwrap();
        assert_eq!(dirs, vec!["a dir".to_owned()]);
        files.sort_unstable();
        assert_eq!(files, vec!["a file".to_owned(), "another file".to_owned()]);

        temp.close().unwrap();
    }
}
