// Conserve backup system.
// Copyright 2016 Martin Pool.


/// Utilities to set up test environments.
///
/// Fixtures that create directories will be automatically deleted when the object
/// is deleted.

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use tempdir;

use super::archive::Archive;
use super::io::write_file_entire;

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

/// A temporary tree for running a test.
///
/// Created in a temporary directory and automatically disposed when done.
pub struct TreeFixture {
    pub root: PathBuf,
    _tempdir: tempdir::TempDir, // held only for cleanup
}

impl TreeFixture {
    pub fn new() -> TreeFixture {
        let tempdir = tempdir::TempDir::new("conserve_TreeFixture").unwrap();
        let root = tempdir.path().to_path_buf();
        TreeFixture {
            _tempdir: tempdir,
            root: root,
        }
    }

    pub fn path(self: &TreeFixture) -> &Path {
        &self.root
    }

    pub fn create_file(self: &TreeFixture, relative_path: &str) {
        let full_path = self.root.join(relative_path);
        write_file_entire(&full_path, b"contents").unwrap();
    }

    pub fn create_dir(self: &TreeFixture, relative_path: &str) {
        fs::create_dir(self.root.join(relative_path)).unwrap();
    }

    #[cfg(unix)]
    pub fn create_symlink(self: &TreeFixture, relative_path: &str, target: &str) {
        use std::os::unix::fs as unix_fs;

        unix_fs::symlink(target, self.root.join(relative_path)).unwrap();
    }
}
