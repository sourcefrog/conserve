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


pub fn restore_tree(
    stored_tree: &StoredTree,
    destination: &Path,
    options: &RestoreOptions,
) -> Result<()> {
    if !options.force_overwrite {
        require_empty_destination(destination)?;
    };
    for entry in stored_tree.index_iter(&options.excludes)? {
        // TODO: Continue even if one fails
        restore_one(
            &stored_tree,
            &entry?,
            destination,
            stored_tree.archive().report(),
            options,
        )?;
    }
    if !stored_tree.is_closed()? {
        warn!(
            "Version {} is incomplete: tree may be truncated",
            stored_tree.band().id()
        );
    }
    Ok(())
}


/// The destination must either not exist, or be an empty directory.
fn require_empty_destination(destination: &Path) -> Result<()> {
    match fs::read_dir(&destination) {
        Ok(mut it) => {
            if it.next().is_some() {
                return Err(
                    ErrorKind::DestinationNotEmpty(destination.to_path_buf()).into(),
                );
            };
        }
        Err(e) => {
            return Err(e.into());
        }
    }
    Ok(())
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
        Kind::Dir => restore_dir(entry, &dest_path, &report),
        Kind::File => restore_file(stored_tree, entry, &dest_path, &report),
        Kind::Symlink => restore_symlink(entry, &dest_path, &report),
        Kind::Unknown => {
            return Err(format!(
                    "file type Unknown shouldn't occur in archive: {:?}",
                    &entry.apath).into());
        }

    }
    // TODO: Restore permissions.
    // TODO: Reset mtime: can probably use lutimes() but it's not in stable yet.
}

fn restore_dir(_entry: &index::Entry, dest: &Path, report: &Report) -> Result<()> {
    report.increment("dir", 1);
    match fs::create_dir(dest) {
        Ok(_) => Ok(()),
        Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn restore_file(
    stored_tree: &StoredTree,
    entry: &index::Entry,
    dest: &Path,
    report: &Report,
) -> Result<()> {
    report.increment("file", 1);
    // Here too we write a temporary file and then move it into place: so the
    // file under its real name only appears
    let mut af = AtomicFile::new(dest)?;
    for bytes in stored_tree.file_contents(entry)? {
        af.write(bytes?.as_slice())?;
    }
    af.close(&report)
}

#[cfg(unix)]
fn restore_symlink(entry: &index::Entry, dest: &Path, report: &Report) -> Result<()> {
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
fn restore_symlink(entry: &index::Entry, _dest: &Path, report: &Report) -> Result<()> {
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
        let restore_archive = Archive::open(af.path(), &restore_report).unwrap();
        let st = StoredTree::open(&restore_archive, &None).unwrap();
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
        let st = StoredTree::open(&a, &Some(BandId::new(&[0]))).unwrap();
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
            &StoredTree::open(&af, &None).unwrap(),
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
        let st = StoredTree::open(&restore_archive, &None).unwrap();
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
        let st = StoredTree::open(&restore_archive, &None).unwrap();
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
