// Conserve backup system.
// Copyright 2016 Martin Pool.


/// Utilities to set up test environments.

use std::path::PathBuf;

use tempdir;

use super::io::write_file_entire;

/// A temporary tree for running a test.
///
/// Created in a temporary directory and automatically disposed when done.
pub struct TreeFixture {
    pub root: PathBuf,
    #[allow(unused)] tempdir: tempdir::TempDir, // held only for cleanup
}

impl TreeFixture {
    pub fn new() -> TreeFixture {
        let tempdir = tempdir::TempDir::new("conserve_TreeFixture").unwrap();
        let root = tempdir.path().to_path_buf();
        TreeFixture {
            tempdir: tempdir,
            root: root,
        }
    }

    pub fn create_file(self: &TreeFixture, relative_path: &str) {
        let full_path = self.root.join(relative_path);
        write_file_entire(&full_path, "contents".as_bytes()).unwrap();
    }
}
