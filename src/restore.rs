// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Restore from the archive to the filesystem.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::entry::Entry;
use crate::io::{directory_is_empty, ensure_dir_exists};
use crate::stats::CopyStats;
use crate::*;

/// A write-only tree on the filesystem, as a restore destination.
#[derive(Debug)]
pub struct RestoreTree {
    path: PathBuf,
}

impl RestoreTree {
    /// Create a RestoreTree.
    ///
    /// The destination must either not yet exist, or be an empty directory.
    pub fn create<P: Into<PathBuf>>(path: P) -> Result<RestoreTree> {
        let path = path.into();
        match ensure_dir_exists(&path).and_then(|()| directory_is_empty(&path)) {
            Err(source) => Err(Error::Restore { path, source }),
            Ok(true) => Ok(RestoreTree { path }),
            Ok(false) => Err(Error::DestinationNotEmpty { path }),
        }
    }

    /// Create a RestoreTree, even if the destination directory is not empty.
    pub fn create_overwrite(path: &Path) -> Result<RestoreTree> {
        Ok(RestoreTree {
            path: path.to_path_buf(),
        })
    }

    fn rooted_path(&self, apath: &Apath) -> PathBuf {
        // Remove initial slash so that the apath is relative to the destination.
        self.path.join(&apath[1..])
    }
}

impl tree::WriteTree for RestoreTree {
    fn finish(self) -> Result<CopyStats> {
        // Live tree doesn't need to be finished.
        Ok(CopyStats::default())
    }

    fn copy_dir<E: Entry>(&mut self, entry: &E) -> Result<()> {
        let path = self.rooted_path(entry.apath());
        match fs::create_dir(&path) {
            Ok(()) => Ok(()),
            Err(source) => {
                if source.kind() == io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(Error::Restore { path, source })
                }
            }
        }
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file<R: ReadTree>(
        &mut self,
        source_entry: &R::Entry,
        from_tree: &R,
    ) -> Result<CopyStats> {
        // TODO: Restore permissions.
        // TODO: Reset mtime: can probably use https://docs.rs/utime/0.2.2/utime/
        // TODO: For restore, maybe not necessary to rename into place, and
        // we could just write directly.
        let path = self.rooted_path(source_entry.apath());
        let restore_err = |source| Error::Restore {
            path: path.clone(),
            source,
        };
        let mut af = AtomicFile::new(&path).map_err(restore_err)?;
        // TODO: Read one block at a time: don't pull all the contents into memory.
        let content = &mut from_tree.file_contents(&source_entry)?;
        let bytes_copied = std::io::copy(content, &mut af).map_err(restore_err)?;
        af.close().map_err(restore_err)?;
        // TODO: Accumulate stats.
        Ok(CopyStats {
            uncompressed_bytes: bytes_copied,
            ..CopyStats::default()
        })
    }

    #[cfg(unix)]
    fn copy_symlink<E: Entry>(&mut self, entry: &E) -> Result<()> {
        use std::os::unix::fs as unix_fs;
        if let Some(ref target) = entry.symlink_target() {
            let path = self.rooted_path(entry.apath());
            unix_fs::symlink(target, &path).map_err(|source| Error::Restore { path, source })?;
        } else {
            // TODO: Treat as an error.
            ui::problem(&format!("No target in symlink entry {}", entry.apath()));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn copy_symlink<E: Entry>(&mut self, entry: &E) -> Result<()> {
        // TODO: Add a test with a canned index containing a symlink, and expect
        // it cannot be restored on Windows and can be on Unix.
        ui::problem(&format!(
            "Can't restore symlinks on non-Unix: {}",
            entry.apath()
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spectral::prelude::*;

    use super::super::*;
    use crate::test_fixtures::{ScratchArchive, TreeFixture};

    #[test]
    pub fn simple_restore() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();

        let restore_archive = Archive::open(af.path()).unwrap();
        let st = StoredTree::open_last(&restore_archive).unwrap();
        let rt = RestoreTree::create(destdir.path().to_owned()).unwrap();
        let stats = copy_tree(&st, rt, &CopyOptions::default()).unwrap();

        assert_eq!(stats.files, 3);

        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("hello2")).is_a_file();
        assert_that(&dest.join("subdir").as_path()).is_a_directory();
        assert_that(&dest.join("subdir").join("subfile").as_path()).is_a_file();
        if SYMLINKS_SUPPORTED {
            let dest = fs::read_link(&dest.join("link")).unwrap();
            assert_eq!(dest.to_string_lossy(), "target");
        }

        // TODO: Test restore empty file.
        // TODO: Test file contents are as expected.
        // TODO: Test restore of larger files.
    }

    #[test]
    fn restore_named_band() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        let a = Archive::open(af.path()).unwrap();
        let st = StoredTree::open_version(&a, &BandId::new(&[0])).unwrap();
        let rt = RestoreTree::create(destdir.path().to_owned()).unwrap();
        let stats = copy_tree(&st, rt, &CopyOptions::default()).unwrap();
        // Does not have the 'hello2' file added in the second version.
        assert_eq!(stats.files, 2);
    }

    #[test]
    pub fn decline_to_overwrite() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        destdir.create_file("existing");
        let restore_err_str = RestoreTree::create(destdir.path().to_owned())
            .unwrap_err()
            .to_string();
        assert_that(&restore_err_str).contains(&"Destination directory not empty");
    }

    #[test]
    pub fn forced_overwrite() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        destdir.create_file("existing");

        let restore_archive = Archive::open(af.path()).unwrap();
        let rt = RestoreTree::create_overwrite(destdir.path()).unwrap();
        let st = StoredTree::open_last(&restore_archive).unwrap();
        let stats = copy_tree(&st, rt, &CopyOptions::default()).unwrap();
        assert_eq!(stats.files, 3);
        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("existing").as_path()).is_a_file();
    }

    #[test]
    pub fn exclude_files() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        let restore_archive = Archive::open(af.path()).unwrap();
        let st = StoredTree::open_last(&restore_archive)
            .unwrap()
            .with_excludes(excludes::from_strings(&["/**/subfile"]).unwrap());
        let rt = RestoreTree::create_overwrite(destdir.path()).unwrap();
        let stats = copy_tree(&st, rt, &CopyOptions::default()).unwrap();

        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("hello2")).is_a_file();
        assert_that(&dest.join("subdir").as_path()).is_a_directory();
        assert_eq!(stats.files, 2);
    }
}
