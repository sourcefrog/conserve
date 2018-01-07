// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Restore from the archive to the filesystem.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::*;
use super::entry::Entry;
use super::tree::WriteTree;

use globset::GlobSet;

/// Options for Restore operation.
#[derive(Debug)]
pub struct RestoreOptions {
    force_overwrite: bool,
    excludes: GlobSet,
}


impl RestoreOptions {
    pub fn default() -> Self {
        RestoreOptions {
            force_overwrite: false,
            excludes: excludes::excludes_nothing(),
        }
    }

    pub fn with_excludes(self, excludes: GlobSet) -> Self {
        RestoreOptions {
            excludes: excludes,
            ..self
        }
    }

    pub fn force_overwrite(self, f: bool) -> RestoreOptions {
        RestoreOptions {
            force_overwrite: f,
            ..self
        }
    }
}


/// A write-only tree on the filesystem, as a restore destination.
#[derive(Debug)]
struct RestoreTree {
    path: PathBuf,
    report: Report,
}


impl RestoreTree {
    pub fn create(path: &Path, report: &Report) -> Result<RestoreTree> {
        require_empty_destination(path)?;
        Self::create_overwrite(path, report)
    }

    pub fn create_overwrite(path: &Path, report: &Report) -> Result<RestoreTree> {
        Ok(RestoreTree {
            path: path.to_path_buf(),
            report: report.clone(),
        })
    }

    fn restore_one(&mut self, stored_tree: &StoredTree, entry: &IndexEntry) -> Result<()> {
        // TODO: Unify this with make_backup into a generic tree-copier.
        if !Apath::is_valid(&entry.apath) {
            return Err(format!("invalid apath {:?}", &entry.apath).into());
        }
        info!("Restore {:?}", &entry.apath);
        match entry.kind() {
            Kind::Dir => self.write_dir(entry),
            Kind::File => self.write_file(entry, &mut stored_tree.file_contents(entry)?),
            Kind::Symlink => self.write_symlink(entry),
            Kind::Unknown => {
                return Err(format!(
                        "file type Unknown shouldn't occur in archive: {:?}",
                        &entry.apath).into());
            }
        }
        // TODO: Restore permissions.
        // TODO: Reset mtime: can probably use lutimes() but it's not in stable yet.
    }

    fn entry_path(&self, entry: &Entry) -> PathBuf {
        // Remove initial slash so that the apath is relative to the destination.
        self.path.join(&entry.apath().to_string()[1..])
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
        self.report.increment("file", 1);
        let mut af = AtomicFile::new(&self.entry_path(entry))?;
        std::io::copy(content, &mut af)?;
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
            warn!("No target in symlink entry {}", entry.apath());
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn write_symlink(&mut self, entry: &Entry) -> Result<()> {
        // TODO: Add a test with a canned index containing a symlink, and expect
        // it cannot be restored on Windows and can be on Unix.
        warn!("Can't restore symlinks on non-Unix: {}", entry.apath());
        self.report.increment("skipped.unsupported_file_kind", 1);
        Ok(())
    }
}


pub fn restore_tree(stored_tree: &StoredTree, destination: &Path, options: &RestoreOptions)
    -> Result<()> {
    let report = stored_tree.archive().report();
    let mut rt = if options.force_overwrite {
        RestoreTree::create_overwrite(destination, report)
    } else {
        RestoreTree::create(destination, report)
    }?;
    for entry in stored_tree.iter_entries(&options.excludes)? {
        // TODO: Continue even if one fails
        rt.restore_one(&stored_tree, &entry?)?;
    }
    rt.finish()
}


/// The destination must either not exist, or be an empty directory.
fn require_empty_destination(destination: &Path) -> Result<()> {
    match fs::read_dir(&destination) {
        Ok(mut it) => {
            if it.next().is_some() {
                Err(
                    ErrorKind::DestinationNotEmpty(destination.to_path_buf()).into(),
                )
            } else {
                Ok(())
            }
        }
        Err(e) => {
            match e.kind() {
                io::ErrorKind::NotFound => Ok(()),
                _ => Err(e.into()),
            }
        }
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
        restore_tree(&st, destdir.path(), &RestoreOptions::default()).unwrap();

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
        let options = RestoreOptions::default();
        restore_tree(&st, destdir.path(), &options).unwrap();
        // Does not have the 'hello2' file added in the second version.
        assert_eq!(2, restore_report.get_count("file"));
    }

    #[test]
    pub fn decline_to_overwrite() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        destdir.create_file("existing");
        let restore_err_str = restore_tree(
            &StoredTree::open_last(&af).unwrap(),
            destdir.path(),
            &RestoreOptions::default(),
        ).unwrap_err()
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
        let options = RestoreOptions::default().force_overwrite(true);
        let st = StoredTree::open_last(&restore_archive).unwrap();
        restore_tree(&st, destdir.path(), &options).unwrap();

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
        let st = StoredTree::open_last(&restore_archive).unwrap();
        let options = RestoreOptions::default().with_excludes(
            excludes::from_strings(
                &["/**/subfile"],
            ).unwrap(),
        );
        restore_tree(&st, destdir.path(), &options).unwrap();

        assert_eq!(2, restore_report.borrow_counts().get_count("file"));
        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("hello2")).is_a_file();
        assert_that(&dest.join("subdir").as_path()).is_a_directory();
    }
}
