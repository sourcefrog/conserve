// Conserve backup system.
// Copyright 2016 Martin Pool.


/// Utilities to set up test environments.
///
/// Fixtures that create directories will be automatically deleted when the object
/// is deleted.

use std::ops::Deref;
use std::path::{Path};

use tempdir;

use super::{Archive};

/// A temporary archive.
pub struct ScratchArchive {
    _tempdir: tempdir::TempDir, // held only for cleanup
    archive: Archive,
}

impl ScratchArchive {
    pub fn new() -> ScratchArchive {
        let tempdir = tempdir::TempDir::new("conserve_ScratchArchive").unwrap();
        let arch_dir = tempdir.path().join("archive");
        let archive = Archive::init(&arch_dir).unwrap();
        ScratchArchive {
            _tempdir: tempdir,
            archive: archive,
        }
    }

    pub fn path(&self) -> &Path {
        self.archive.path()
    }

    #[allow(unused)]
    pub fn archive_dir_str(self: &ScratchArchive) -> &str {
        self.archive.path().to_str().unwrap()
    }
}

impl Deref for ScratchArchive {
    type Target = Archive;

    /// ScratchArchive can be directly used as an archive.
    fn deref(&self) -> &Archive {
        &self.archive
    }
}
