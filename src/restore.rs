// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Restore from the archive to the filesystem.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::entry::Entry;
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
        require_empty_destination(path)?;
        Self::create_overwrite(path, report)
    }

    /// Create a RestoreTree, even if the destination directory is not empty.
    pub fn create_overwrite(path: &Path, report: &Report) -> Result<RestoreTree> {
        Ok(RestoreTree {
            path: path.to_path_buf(),
            report: report.clone(),
        })
    }

    fn entry_path(&self, entry: &Entry) -> PathBuf {
        // Remove initial slash so that the apath is relative to the destination.
        self.path.join(&entry.apath()[1..])
    }
}

impl tree::WriteTree for RestoreTree {
    fn finish(&mut self) -> Result<()> {
        // Live tree doesn't need to be finished.
        Ok(())
    }

    fn write_dir(&mut self, entry: &Entry) -> Result<()> {
        self.report.increment("dir", 1);
        match fs::create_dir(self.entry_path(entry)) {
            Ok(_) => Ok(()),
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn write_file(&mut self, entry: &Entry, content: &mut std::io::Read) -> Result<()> {
        // TODO: Restore permissions.
        // TODO: Reset mtime: can probably use lutimes() but it's not in stable yet.
        // TODO: For restore, maybe not necessary to rename into place, and
        // we could just write directly.
        self.report.increment("file", 1);
        let mut af = AtomicFile::new(&self.entry_path(entry))?;
        let bytes = std::io::copy(content, &mut af)?;
        self.report.increment_size(
            "file.bytes",
            Sizes {
                uncompressed: bytes,
                compressed: 0,
            },
        );
        af.close(&self.report)
    }

    #[cfg(unix)]
    fn write_symlink(&mut self, entry: &Entry) -> Result<()> {
        use std::os::unix::fs as unix_fs;
        self.report.increment("symlink", 1);
        if let Some(ref target) = entry.symlink_target() {
            unix_fs::symlink(target, self.entry_path(entry))?;
        } else {
            // TODO: Treat as an error.
            self.report
                .problem(&format!("No target in symlink entry {}", entry.apath()));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn write_symlink(&mut self, entry: &Entry) -> Result<()> {
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

/// The destination must either not exist, or be an empty directory.
// TODO: Merge with or just use require_empty_directory?
fn require_empty_destination(dest: &Path) -> Result<()> {
    match fs::read_dir(&dest) {
        Ok(mut it) => {
            if it.next().is_some() {
                Err(Error::DestinationNotEmpty(dest.to_path_buf()))
            } else {
                Ok(())
            }
        }
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => Ok(()),
            _ => Err(e.into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use spectral::prelude::*;

    use super::super::*;
    use test_fixtures::{ScratchArchive, TreeFixture};

    #[test]
    pub fn simple_restore() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();

        let restore_report = Report::new();
        let restore_archive = Archive::open(af.path(), &restore_report).unwrap();
        let st = StoredTree::open_last(&restore_archive).unwrap();
        let mut rt = RestoreTree::create(destdir.path(), &restore_report).unwrap();
        copy_tree(&st, &mut rt).unwrap();

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
        copy_tree(&st, &mut rt).unwrap();
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
        copy_tree(&st, &mut rt).unwrap();

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
        copy_tree(&st, &mut rt).unwrap();

        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("hello2")).is_a_file();
        assert_that(&dest.join("subdir").as_path()).is_a_directory();
        assert_eq!(2, restore_report.borrow_counts().get_count("file"));
    }
}
