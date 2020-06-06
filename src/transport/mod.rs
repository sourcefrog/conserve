// Copyright 2020 Martin Pool.

//! Filesystem abstraction to read and write local and remote archives.
//!
//! Transport operations return std::io::Result to reflect their narrower focus.

use std::io;

use crate::kind::Kind;

pub mod local;

/// Facade to read from an archive.
///
/// This supports operations that are common across local filesystems, SFTP, and cloud storage, and
/// that are intended to be sufficient to efficiently implement the Conserve format.
///
/// A transport has a root location, which will typically be the top directory of the Archive.
/// Below that point everything is accessed with a relative path, expressed as a PathBuf.
///
/// All Transports must be Send and Clone, so that new instances can be created within parallel
/// threads. They need not be Sync. Although the transports are Clone, it's OK for them to hold
/// state (such as a network connection) that makes reusing them cheaper than cloning.
///
/// Files in Conserve archives have bounded size and fit in memory so this does not need to
/// support streaming or partial reads and writes.
pub trait TransportRead: Send + Clone {
    /// Read the contents of a directory under this transport, without recursing down.
    ///
    /// Returned entries are in arbitrary order and may be interleaved with errors.
    ///
    /// The result should not contain entries for "." and "..".
    fn read_dir(
        &mut self,
        path: &str,
    ) -> io::Result<Box<dyn Iterator<Item = io::Result<TransportEntry>>>>;

    /// Get one complete file.
    ///
    /// Files in the archive are of bounded size, so it's OK to always read them entirely into
    /// memory, and this is simple to support on all implementations.
    fn read_file(&mut self, path: &str) -> io::Result<&[u8]>;
}

/// Facade to both read and write an archive.
pub trait TransportWrite: TransportRead {
    /// Create a directory.
    ///
    /// If the directory already exists, this should be an error, but if that's not supported
    /// by the underlying transport it may just succeed.
    fn make_dir(&mut self, apath: &str) -> io::Result<()>;

    /// Write a complete file.
    ///
    /// As much as possible, the file should be written atomically so that it is only visible with
    /// the complete content.
    fn write_file(&mut self, apath: &str, content: &[u8]) -> io::Result<()>;
}

/// A directory entry read from a transport.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TransportEntry {
    /// Path relative to the transport root.
    relpath: String,
    kind: Kind,
}
