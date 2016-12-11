//! Restore from the archive to the filesystem.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{Archive, Band, BlockDir, Report};
use super::apath;
use super::errors::*;
use super::index;
use super::io::AtomicFile;

/// Restore operation.
///
/// Call `from_archive_path` then `run`.
pub struct Restore {
    archive: Archive,
    report: Report,
    destination: PathBuf,
    force_overwrite: bool,
}


impl Restore {
    pub fn new(archive: &Archive, destination: &Path, report: &Report) -> Restore {
        Restore {
            archive: archive.clone(),
            report: report.clone(),
            destination: destination.to_path_buf(),
            force_overwrite: false,
        }
    }

    pub fn force_overwrite(mut self, force: bool) -> Restore {
        self.force_overwrite = force;
        self
    }

    pub fn run(mut self) -> Result<()> {
        let band_id = try!(self.archive.last_band_id());
        let band = try!(Band::open(self.archive.path(), &band_id, &self.report));
        let block_dir = band.block_dir();

        if !self.force_overwrite {
            if let Ok(mut it) = fs::read_dir(&self.destination) {
                if it.next().is_some() {
                    return Err(ErrorKind::DestinationNotEmpty(self.destination).into());
                }
            }
        }
        for entry in try!(band.index_iter(&self.report)) {
            let entry = try!(entry);
            // TODO: Continue even if one fails
            try!(self.restore_one(&block_dir, &entry));
        }
        // TODO: Warn if band is incomplete
        Ok(())
    }

    fn restore_one(&mut self, block_dir: &BlockDir, entry: &index::Entry) -> Result<()> {
        // Remove initial slash so that the apath is relative to the destination.
        if !apath::valid(&entry.apath) {
            return Err(format!("invalid apath {:?}", &entry.apath).into());
        }
        let dest_path = self.destination.join(&entry.apath[1..]);
        // info!("restore {:?} to {:?}", &entry.apath, &dest_path);
        match entry.kind {
            index::IndexKind::Dir => self.restore_dir(entry, &dest_path),
            index::IndexKind::File => self.restore_file(block_dir, entry, &dest_path),
            index::IndexKind::Symlink => self.restore_symlink(entry, &dest_path),
        }
        // TODO: Restore permissions.
        // TODO: Reset mtime: can probably use lutimes() but it's not in stable yet.
    }

    fn restore_dir(&mut self, _entry: &index::Entry, dest: &Path) -> Result<()> {
        self.report.increment("dir", 1);
        match fs::create_dir(dest) {
            Ok(_) => Ok(()),
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn restore_file(&mut self, block_dir: &BlockDir, entry: &index::Entry, dest: &Path) -> Result<()> {
        self.report.increment("file", 1);
        // Here too we write a temporary file and then move it into place: so the file
        // under its real name only appears
        let mut af = try!(AtomicFile::new(dest));
        for addr in &entry.addrs {
            let block_vec = try!(block_dir.get(&addr, &self.report));
            try!(io::copy(&mut block_vec.as_slice(), &mut af));
        }
        af.close(&self.report)
    }

    #[cfg(unix)]
    fn restore_symlink(&mut self, entry: &index::Entry, dest: &Path) -> Result<()> {
        use std::os::unix::fs as unix_fs;
        self.report.increment("symlink", 1);
        if let Some(ref target) = entry.target {
            unix_fs::symlink(target, dest).unwrap();
        } else {
            warn!("No target in symlink entry {}", entry.apath);
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn restore_symlink(&mut self, entry: &index::Entry, _dest: &Path) -> Result<()> {
        // TODO: Add a test with a canned index containing a symlink, and expect
        // it cannot be restored on Windows and can be on Unix.
        warn!("Can't restore symlinks on Windows: {}", entry.apath);
        self.report.increment("skipped.unsupported_file_kind", 1);
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use std::fs;

    use spectral::prelude::*;

    use super::super::SYMLINKS_SUPPORTED;
    use super::Restore;
    use super::super::backup::backup;
    use super::super::report::Report;
    use super::super::testfixtures::ScratchArchive;
    use conserve_testsupport::TreeFixture;

    fn setup_archive() -> ScratchArchive {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_file("hello");
        srcdir.create_dir("subdir");
        srcdir.create_file("subdir/subfile");
        if SYMLINKS_SUPPORTED {
            srcdir.create_symlink("link", "target");
        }

        let backup_report = Report::new();
        backup(af.path(), srcdir.path(), &backup_report).unwrap();
        af
    }

    #[test]
    pub fn simple_restore() {
        let af = setup_archive();
        let destdir = TreeFixture::new();
        let restore_report = Report::new();
        Restore::new(&af, destdir.path(), &restore_report).run().unwrap();

        assert_eq!(2, restore_report.borrow_counts().get_count("file"));
        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
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
    pub fn decline_to_overwrite() {
        let af = setup_archive();
        let destdir = TreeFixture::new();
        destdir.create_file("existing");
        let restore_report = Report::new();
        let restore_err = Restore::new(&af, destdir.path(), &restore_report).run().unwrap_err();
        let restore_err_str = restore_err.to_string();
        assert_that(&restore_err_str).contains(&"Destination directory not empty");
    }

    #[test]
    pub fn forced_overwrite() {
        let af = setup_archive();
        let destdir = TreeFixture::new();
        destdir.create_file("existing");
        let restore_report = Report::new();
        Restore::new(&af, destdir.path(), &restore_report).force_overwrite(true)
            .run().unwrap();

        assert_eq!(2, restore_report.borrow_counts().get_count("file"));
        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("existing").as_path()).is_a_file();
    }
}
