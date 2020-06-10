// Copyright 2020 Martin Pool.

//! Filesystem abstraction to read and write local and remote archives.
//!
//! Transport operations return std::io::Result to reflect their narrower focus.

use std::io;
use std::path::Path;

use crate::kind::Kind;
use crate::Result;

pub mod local;

/// Facade to read from an archive.
///
/// This supports operations that are common across local filesystems, SFTP, and cloud storage, and
/// that are intended to be sufficient to efficiently implement the Conserve format.
///
/// A transport has a root location, which will typically be the top directory of the Archive.
/// Below that point everything is accessed with a relative path, expressed as a PathBuf.
///
/// All Transports must be `Send + Sync`, so they can be passed across or shared across threads.
///
/// TransportRead is object-safe so can be used as `dyn TransportRead`.
///
/// Files in Conserve archives have bounded size and fit in memory so this does not need to
/// support streaming or partial reads and writes.
pub trait TransportRead: Send + Sync + std::fmt::Debug {
    /// Read the contents of a directory under this transport, without recursing down.
    ///
    /// Returned entries are in arbitrary order and may be interleaved with errors.
    ///
    /// The result should not contain entries for "." and "..".
    fn read_dir(&self, path: &str) -> io::Result<Box<dyn Iterator<Item = io::Result<DirEntry>>>>;

    /// Get one complete file into a caller-provided buffer.
    ///
    /// Files in the archive are of bounded size, so it's OK to always read them entirely into
    /// memory, and this is simple to support on all implementations.
    fn read_file(&self, path: &str, out_buf: &mut Vec<u8>) -> io::Result<()>;

    /// Check if an entry exists.
    fn exists(&self, path: &str) -> io::Result<bool>;

    /// Clone this object into a new box.
    fn box_clone(&self) -> Box<dyn TransportRead>;
}

impl Clone for Box<dyn TransportRead> {
    fn clone(&self) -> Box<dyn TransportRead> {
        self.box_clone()
    }
}

impl dyn TransportRead {
    pub fn new(s: &str) -> Result<Box<dyn TransportRead>> {
        // TODO: Recognize URL-style strings.
        Ok(Box::new(local::LocalTransport::new(&Path::new(s))))
    }
}

/// Facade to both read and write an archive.
pub trait TransportWrite: TransportRead {
    /// Create a directory, if it does not exist.
    ///
    /// If the directory already exists, it's not an error.
    ///
    /// This function does not create missing parent directories.
    fn create_dir(&mut self, relpath: &str) -> io::Result<()>;

    /// Write a complete file.
    ///
    /// As much as possible, the file should be written atomically so that it is only visible with
    /// the complete content. On a local filesystem the content is written to a temporary file and
    /// then renamed.
    ///
    /// If a temporary file is used, the name should start with `crate::TMP_PREFIX`.
    fn write_file(&mut self, relpath: &str, content: &[u8]) -> io::Result<()>;

    /// Clone this object into a new box.
    fn box_clone_write(&self) -> Box<dyn TransportWrite>;

    /// Make a new transport addressing a subdirectory.
    fn sub_transport(&self, relpath: &str) -> Box<dyn TransportWrite>;
}

impl Clone for Box<dyn TransportWrite> {
    fn clone(&self) -> Box<dyn TransportWrite> {
        self.box_clone_write()
    }
}

impl dyn TransportWrite {
    pub fn new(s: &str) -> Result<Box<dyn TransportWrite>> {
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
