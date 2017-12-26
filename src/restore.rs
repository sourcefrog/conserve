// Copyright 2015, 2016, 2017 Martin Pool.

//! Restore from the archive to the filesystem.

use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;

use super::*;
use super::index;

use globset::GlobSet;

/// Options for Restore operation.
#[derive(Debug)]
pub struct RestoreOptions {
    force_overwrite: bool,
    band_id: Option<BandId>,
    excludes: GlobSet,
}


impl RestoreOptions {
    pub fn default() -> Self {
        RestoreOptions {
            force_overwrite: false,
            band_id: None,
            excludes: excludes::excludes_nothing()
        }
    }

    pub fn with_excludes(self, exclude: Vec<&str>) -> Result<Self> {
        Ok(RestoreOptions {
            excludes: excludes::from_strings(exclude)?,
            ..self
        })
    }

    pub fn force_overwrite(self, f: bool) -> RestoreOptions {
        RestoreOptions {
            force_overwrite: f,
            ..self
        }
    }

    pub fn band_id(self, b: Option<BandId>) -> RestoreOptions {
        RestoreOptions { band_id: b, ..self }
    }

    /// Restore a version from the archive.
    ///
    /// This will warn, but not fail, if the version is incomplete: this might
    /// mean only part of the source tree is copied back.
    pub fn restore(
        &self,
        archive: &Archive,
        destination: &Path,
        report: &Report,
    ) -> Result<()> {
        let options = &self;
        let stored_tree = archive.stored_tree(&options.band_id)?;

        if !options.force_overwrite {
            if let Ok(mut it) = fs::read_dir(&destination) {
                if it.next().is_some() {
                    return Err(
                        ErrorKind::DestinationNotEmpty(
                            destination.to_path_buf(),
                        ).into(),
                    );
                };
            }
            // TODO: Propagate error from readdir?
        };
        for entry in stored_tree.band().index_iter(&self.excludes, &report)? {
            // TODO: Continue even if one fails
            restore_one(
                &stored_tree,
                &entry?,
                destination,
                report,
                options,
            )?;
        }
        if !stored_tree.is_closed()? {
            warn!("Version {} is incomplete: tree may be truncated", stored_tree.band().id());
        }
        Ok(())
    }
}


fn restore_one(
    stored_tree: &StoredTree,
    entry: &index::Entry,
    destination: &Path,
    report: &Report,
    _options: &RestoreOptions,
) -> Result<()> {
    // Remove initial slash so that the apath is relative to the destination.
    if !Apath::is_valid(&entry.apath) {
        return Err(format!("invalid apath {:?}", &entry.apath).into());
    }
    let dest_path = destination.join(&entry.apath[1..]);
    info!("Restore {:?} to {:?}", &entry.apath, &dest_path);
    match entry.kind {
        index::IndexKind::Dir => restore_dir(entry, &dest_path, &report),
        index::IndexKind::File => {
            restore_file(stored_tree, entry, &dest_path, &report)
        }
        index::IndexKind::Symlink => {
            restore_symlink(entry, &dest_path, &report)
        }
    }
    // TODO: Restore permissions.
    // TODO: Reset mtime: can probably use lutimes() but it's not in stable yet.
}

fn restore_dir(
    _entry: &index::Entry,
    dest: &Path,
    report: &Report,
) -> Result<()> {
    report.increment("dir", 1);
    match fs::create_dir(dest) {
        Ok(_) => Ok(()),
        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn restore_file(stored_tree: &StoredTree, entry: &index::Entry, dest: &Path, report: &Report) -> Result<()> {
    report.increment("file", 1);
    // Here too we write a temporary file and then move it into place: so the
    // file under its real name only appears
    let mut af = AtomicFile::new(dest)?;
    for bytes in stored_tree.file_contents(entry, report)? {
        af.write(bytes?.as_slice())?;
    }
    af.close(&report)
}

#[cfg(unix)]
fn restore_symlink(
    entry: &index::Entry,
    dest: &Path,
    report: &Report,
) -> Result<()> {
    use std::os::unix::fs as unix_fs;
    report.increment("symlink", 1);
    if let Some(ref target) = entry.target {
        unix_fs::symlink(target, dest).unwrap();
    } else {
        warn!("No target in symlink entry {}", entry.apath);
    }
    Ok(())
}

#[cfg(not(unix))]
fn restore_symlink(
    entry: &index::Entry,
    _dest: &Path,
    report: &Report,
) -> Result<()> {
    // TODO: Add a test with a canned index containing a symlink, and expect
    // it cannot be restored on Windows and can be on Unix.
    warn!("Can't restore symlinks on Windows: {}", entry.apath);
    report.increment("skipped.unsupported_file_kind", 1);
    Ok(())
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
        RestoreOptions::default()
            .restore(&af, destdir.path(), &restore_report)
            .unwrap();

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
        let options =
            RestoreOptions::default().band_id(Some(BandId::new(&[0])));
        options
            .restore(&af, destdir.path(), &restore_report)
            .unwrap();
        // Does not have the 'hello2' file added in the second version.
        assert_eq!(2, restore_report.get_count("file"));
    }

    #[test]
    pub fn decline_to_overwrite() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        destdir.create_file("existing");
        let restore_report = Report::new();
        let options = RestoreOptions::default();
        let restore_err = options
            .restore(&af, destdir.path(), &restore_report)
            .unwrap_err();
        let restore_err_str = restore_err.to_string();
        assert_that(&restore_err_str).contains(
            &"Destination directory not empty",
        );
    }

    #[test]
    pub fn forced_overwrite() {
        let af = ScratchArchive::new();
        af.store_two_versions();
        let destdir = TreeFixture::new();
        destdir.create_file("existing");
        let restore_report = Report::new();
        let options = RestoreOptions::default().force_overwrite(true);
        options
            .restore(&af, destdir.path(), &restore_report)
            .unwrap();

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
        RestoreOptions::default()
            .with_excludes(vec!["/**/subfile"]).unwrap()
            .restore(&af, destdir.path(), &restore_report)
            .unwrap();

        assert_eq!(2, restore_report.borrow_counts().get_count("file"));
        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("hello2")).is_a_file();
        assert_that(&dest.join("subdir").as_path()).is_a_directory();
    }
}
