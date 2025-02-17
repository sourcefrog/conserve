// Conserve backup system.
// Copyright 2016, 2017, 2018, 2019 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

/// Utilities to set up test environments.
///
/// Fixtures that create directories will be automatically deleted when the object
/// is deleted.
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::monitor::test::TestMonitor;
use crate::transport::Transport;
use crate::*;

/// A temporary archive, deleted when it goes out of scope.
///
/// The ScratchArchive can be treated as an Archive.
pub struct ScratchArchive {
    #[allow(dead_code)]
    tempdir: TempDir, // held only for cleanup
    archive: Archive,
    archive_path: PathBuf,
}

impl ScratchArchive {
    pub fn new() -> ScratchArchive {
        let tempdir = TempDir::new().unwrap();
        let archive_path = tempdir.path().join("archive");
        let archive = Archive::create_path(&archive_path).unwrap();
        ScratchArchive {
            tempdir,
            archive,
            archive_path,
        }
    }

    pub fn path(&self) -> &Path {
        &self.archive_path
    }

    pub async fn setup_incomplete_empty_band(&self) {
        Band::create(&self.archive).await.unwrap();
    }

    pub async fn store_two_versions(&self) {
        let srcdir = TreeFixture::new();
        srcdir.create_file("hello");
        srcdir.create_dir("subdir");
        srcdir.create_file("subdir/subfile");
        if SYMLINKS_SUPPORTED {
            srcdir.create_symlink("link", "target");
        }

        let options = &BackupOptions::default();
        backup(&self.archive, srcdir.path(), options, TestMonitor::arc())
            .await
            .unwrap();

        srcdir.create_file("hello2");
        backup(&self.archive, srcdir.path(), options, TestMonitor::arc())
            .await
            .unwrap();
    }

    pub fn transport(&self) -> &Transport {
        self.archive.transport()
    }
}

impl Deref for ScratchArchive {
    type Target = Archive;

    /// ScratchArchive can be directly used as an archive.
    fn deref(&self) -> &Archive {
        &self.archive
    }
}

impl AsRef<Archive> for ScratchArchive {
    fn as_ref(&self) -> &Archive {
        &self.archive
    }
}

impl Default for ScratchArchive {
    fn default() -> Self {
        Self::new()
    }
}

/// A temporary tree for running a test.
///
/// Created in a temporary directory and automatically disposed when done.
pub struct TreeFixture {
    pub root: PathBuf,
    _tempdir: TempDir, // held only for cleanup
}

impl TreeFixture {
    pub fn new() -> TreeFixture {
        let tempdir = TempDir::new().unwrap();
        let root = tempdir.path().to_path_buf();
        TreeFixture {
            _tempdir: tempdir,
            root,
        }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Make a file in the tree, with arbitrary contents. Returns the full path.
    pub fn create_file(&self, relative_path: &str) -> PathBuf {
        self.create_file_with_contents(relative_path, b"contents")
    }

    /// Make a file in the tree, with given contents. Returns the full path.
    pub fn create_file_with_contents(&self, relative_path: &str, contents: &[u8]) -> PathBuf {
        let full_path = self.root.join(relative_path);
        let mut f = fs::File::create(&full_path).unwrap();
        f.write_all(contents).unwrap();
        full_path
    }

    /// Create a file with a specified length. The first bytes of the file are the `prefix` and the remainder is zeros.
    pub fn create_file_of_length_with_prefix(
        &self,
        relative_path: &str,
        length: u64,
        prefix: &[u8],
    ) -> PathBuf {
        let full_path = self.root.join(relative_path);
        let mut f = fs::File::create(&full_path).unwrap();
        f.write_all(prefix).unwrap();
        f.set_len(length).expect("set file length");
        full_path
    }

    /// Create a new subdirectory and return its full path.
    pub fn create_dir(&self, relative_path: &str) -> PathBuf {
        let full_path: PathBuf = self.root.join(relative_path);
        fs::create_dir(&full_path).unwrap();
        full_path
    }

    #[cfg(unix)]
    pub fn create_symlink(&self, relative_path: &str, target: &str) {
        use std::os::unix::fs as unix_fs;

        unix_fs::symlink(target, self.root.join(relative_path)).unwrap();
    }

    /// Symlinks are just not present on Windows.
    #[cfg(windows)]
    pub fn create_symlink(&self, _relative_path: &str, _target: &str) {}

    pub fn live_tree(&self) -> SourceTree {
        // TODO: Maybe allow deref TreeFixture to LiveTree.
        SourceTree::open(self.path()).unwrap()
    }

    #[cfg(unix)]
    pub fn make_file_unreadable(&self, relative_path: &str) {
        use std::fs::File;
        use std::os::unix::fs::PermissionsExt;
        let p = self.root.join(relative_path);
        let f = File::open(&p).unwrap();
        let mut perms = f.metadata().unwrap().permissions();
        perms.set_mode(0o0);
        fs::set_permissions(&p, perms).unwrap();
    }
}

impl Default for TreeFixture {
    fn default() -> Self {
        Self::new()
    }
}

/// Collect apaths from an iterator into a list of string.
///
/// This is more loosely typed but useful for tests.
pub fn entry_iter_to_apath_strings<EntryIter, E>(entry_iter: EntryIter) -> Vec<String>
where
    EntryIter: IntoIterator<Item = E>,
    E: EntryTrait,
{
    entry_iter
        .into_iter()
        .map(|entry| entry.apath().clone().into())
        .collect()
}

/// On Unix, return the gid of a group that the current user is a member of
/// and that is not the primary group. This can be used to test chgrp operations.
///
/// Returns None if there is no such group: if the user is a member of only
/// their primary group, or if the list of groups can't be retrieved, which is
/// always the case on macOS.
#[cfg(target_os = "linux")]
pub fn arbitrary_secondary_group() -> Option<u32> {
    let groups = nix::unistd::getgroups().expect("getgroups");
    let primary_group = nix::unistd::getgid();
    groups
        .iter()
        .find(|gid| **gid != primary_group)
        .map(|gid| gid.as_raw())
}

#[cfg(test)]
mod test {
    #[cfg(target_os = "linux")]
    #[test]
    fn arbitrary_secondary_group_is_found() {
        let gid = super::arbitrary_secondary_group();
        assert!(gid.is_none_or(|gid| gid > 0));
    }
}
