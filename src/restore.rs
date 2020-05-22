// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Restore from the archive to the filesystem.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use snafu::ResultExt;

use super::entry::Entry;
use super::io::{directory_is_empty, ensure_dir_exists};
use super::*;

/// A write-only tree on the filesystem, as a restore destination.
#[derive(Debug)]
pub struct RestoreTree {
    path: PathBuf,
    report: Report,
}

impl RestoreTree {
    /// Create a RestoreTree.
    ///
    /// The destination must either not yet exist, or be an empty directory.
    pub fn create(path: &Path, report: &Report) -> Result<RestoreTree> {
        if ensure_dir_exists(&path)
            .and_then(|()| directory_is_empty(&path))
            .context(errors::Restore {
                path: path.to_path_buf(),
            })?
        {
            Ok(RestoreTree {
                path: path.to_path_buf(),
                report: report.clone(),
            })
        } else {
            errors::DestinationNotEmpty { path }.fail()
        }
    }

    /// Create a RestoreTree, even if the destination directory is not empty.
    pub fn create_overwrite(path: &Path, report: &Report) -> Result<RestoreTree> {
        Ok(RestoreTree {
            path: path.to_path_buf(),
            report: report.clone(),
        })
    }

    fn rooted_path(&self, apath: &Apath) -> PathBuf {
        // Remove initial slash so that the apath is relative to the destination.
        self.path.join(&apath[1..])
    }
}

impl tree::WriteTree for RestoreTree {
    fn finish(&mut self) -> Result<()> {
        // Live tree doesn't need to be finished.
        Ok(())
    }

    fn copy_dir<E: Entry>(&mut self, entry: &E) -> Result<()> {
        self.report.increment("dir", 1);
        let path = self.rooted_path(entry.apath());
        match fs::create_dir(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            e => e.context(errors::Restore { path }),
        }
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file<R: ReadTree>(&mut self, source_entry: &R::Entry, from_tree: &R) -> Result<Sizes> {
        // TODO: Restore permissions.
        // TODO: Reset mtime: can probably use https://docs.rs/utime/0.2.2/utime/
        // TODO: For restore, maybe not necessary to rename into place, and
        // we could just write directly.
        self.report.increment("file", 1);
        let path = self.rooted_path(source_entry.apath());
        let ctx = || errors::Restore { path: path.clone() };
        let mut af = AtomicFile::new(&path).with_context(ctx)?;
        // TODO: Read one block at a time: don't pull all the contents into memory.
        let content = &mut from_tree.file_contents(&source_entry)?;
        let bytes = std::io::copy(content, &mut af).with_context(ctx)?;
        af.close(&self.report).context(errors::Restore { path })?;
        Ok(Sizes {
            uncompressed: bytes,
            compressed: 0,
        })
    }

    #[cfg(unix)]
    fn copy_symlink<E: Entry>(&mut self, entry: &E) -> Result<()> {
        use std::os::unix::fs as unix_fs;
        self.report.increment("symlink", 1);
        if let Some(ref target) = entry.symlink_target() {
            let path = self.rooted_path(entry.apath());
            unix_fs::symlink(target, &path).context(errors::Restore { path })?;
        } else {
            // TODO: Treat as an error.
            self.report
                .problem(&format!("No target in symlink entry {}", entry.apath()));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn copy_symlink<E: Entry>(&mut self, entry: &E) -> Result<()> {
        // TODO: Add a test with a canned index containing a symlink, and expect
        // it cannot be restored on Windows and can be on Unix.
        self.report.problem(&format!(
            "Can't restore symlinks on non-Unix: {}",
            entry.apath()
        ));
        self.report.increment("skipped.unsupported_file_kind", 1);
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

        let restore_report = Report::new();
        let restore_archive = Archive::open(af.path(), &restore_report).unwrap();
        let st = StoredTree::open_last(&restore_archive).unwrap();
        let mut rt = RestoreTree::create(destdir.path(), &restore_report).unwrap();
        copy_tree(&st, &mut rt, &CopyOptions::default()).unwrap();

        assert_eq!(3, restore_report.get_count("file"));
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
        let restore_report = Report::new();
        let a = Archive::open(af.path(), &restore_report).unwrap();
        let st = StoredTree::open_version(&a, &BandId::new(&[0])).unwrap();
        let mut rt = RestoreTree::create(&destdir.path(), &restore_report).unwrap();
        copy_tree(&st, &mut rt, &CopyOptions::default()).unwrap();
        // Does not have the 'hello2' file added in the second version.
        assert_eq!(2, restore_report.get_count("file"));
    }

    #[test]
    pub fn decline_to_overwrite() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        destdir.create_file("existing");
        let restore_err_str = RestoreTree::create(&destdir.path(), &Report::new())
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

        let restore_report = Report::new();
        let restore_archive = Archive::open(af.path(), &restore_report).unwrap();
        let mut rt = RestoreTree::create_overwrite(&destdir.path(), &restore_report).unwrap();
        let st = StoredTree::open_last(&restore_archive).unwrap();
        copy_tree(&st, &mut rt, &CopyOptions::default()).unwrap();

        assert_eq!(3, restore_report.get_count("file"));
        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("existing").as_path()).is_a_file();
    }

    #[test]
    pub fn exclude_files() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        let restore_report = Report::new();
        let restore_archive = Archive::open(af.path(), &restore_report).unwrap();
        let st = StoredTree::open_last(&restore_archive)
            .unwrap()
            .with_excludes(excludes::from_strings(&["/**/subfile"]).unwrap());
        let mut rt = RestoreTree::create_overwrite(&destdir.path(), &restore_report).unwrap();
        copy_tree(&st, &mut rt, &CopyOptions::default()).unwrap();

        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("hello2")).is_a_file();
        assert_that(&dest.join("subdir").as_path()).is_a_directory();
        assert_eq!(2, restore_report.borrow_counts().get_count("file"));
    }
}
