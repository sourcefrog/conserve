// Conserve backup system.
// Copyright 2016 Martin Pool.


/// Utilities to set up test environments.
///
/// Fixtures that create directories will be automatically deleted when the object
/// is deleted.

use std::fs;
use std::path::{Path, PathBuf};

use tempdir;

use super::archive::Archive;
use super::io::write_file_entire;

/// A temporary archive.
pub struct ArchiveFixture {
    _tempdir: tempdir::TempDir, // held only for cleanup
    pub archive: Archive,
}

impl ArchiveFixture {
    pub fn new() -> ArchiveFixture {
        let tempdir = tempdir::TempDir::new("conserve_ArchiveFixture").unwrap();
        let arch_dir = tempdir.path().join("archive");
        let archive = Archive::init(&arch_dir).unwrap();
        ArchiveFixture {
            _tempdir: tempdir,
            archive: archive,
        }
    }

    pub fn archive_dir_str(self: &ArchiveFixture) -> &str {
        self.archive.path().to_str().unwrap()
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
