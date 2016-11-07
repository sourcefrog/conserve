//! Restore from the archive to the filesystem.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::*;
use super::apath;
use super::errors::*;
use super::index;
use super::io::AtomicFile;

/// Restore operation.
pub struct Restore<'a> {
    pub band: Band,
    pub report: &'a Report,
    pub destination: PathBuf,
    pub block_dir: BlockDir,
}

pub fn restore(archive_path: &Path, destination: &Path, report: &Report) -> Result<()> {
    let archive = try!(Archive::open(&archive_path));
    let band_id = archive.last_band_id().unwrap().expect("archive is empty");
    let band = Band::open(archive.path(), &band_id, report).unwrap();
    let block_dir = band.block_dir();
    Restore {
        band: band,
        block_dir: block_dir,
        report: report,
        destination: destination.to_path_buf(),
    }.run()
}

impl<'a> Restore<'a> {
    fn run(mut self) -> Result<()> {
        for entry in try!(self.band.index_iter(self.report)) {
            let entry = try!(entry);
            // TODO: Continue even if one fails
            try!(self.restore_one(&entry));
        }
        Ok(())
    }

    fn restore_one(&mut self, entry: &index::Entry) -> Result<()> {
        // Remove initial slash so that the apath is relative to the destination.
        if !apath::valid(&entry.apath) {
            return Err(format!("invalid apath {:?}", &entry.apath).into());
        }
        let dest_path = self.destination.join(&entry.apath[1..]);
        info!("restore {:?} to {:?}", &entry.apath, &dest_path);
        match entry.kind {
            index::IndexKind::Dir => self.restore_dir(entry, &dest_path),
            index::IndexKind::File => self.restore_file(entry, &dest_path),
            // TODO: Restore symlinks.
            ref k => {
                warn!("unimplemented kind {:?}", k);
                return Ok(())
            },
        }
        // TODO: Restore permissions.
        // TODO: Reset mtime: can probably use lutimes() but it's not in stable yet.
    }

    fn restore_dir(&mut self, _entry: &index::Entry, dest: &Path) -> Result<()> {
        self.report.increment("restore.dir", 1);
        match fs::create_dir(dest) {
            Ok(_) => Ok(()),
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn restore_file(&mut self, entry: &index::Entry, dest: &Path) -> Result<()> {
        self.report.increment("restore.file", 1);
        // Here too we write a temporary file and then move it into place: so the file
        // under its real name only appears
        let mut af = try!(AtomicFile::new(dest));
        for addr in &entry.addrs {
            let block_vec = try!(self.block_dir.get(&addr, self.report));
            try!(io::copy(&mut block_vec.as_slice(), &mut af));
        }
        af.close(self.report)
    }
}



#[cfg(test)]
mod tests {
    use spectral::prelude::*;

    use super::restore;
    use super::super::backup::backup;
    use super::super::report::Report;
    use super::super::testfixtures::ScratchArchive;
    use conserve_testsupport::TreeFixture;

    #[test]
    pub fn simple_restore() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_file("hello");
        srcdir.create_dir("subdir");
        srcdir.create_file("subdir/subfile");

        let report = Report::new();
        backup(af.path(), srcdir.path(), &report).unwrap();

        let destdir = TreeFixture::new();
        restore(af.path(), destdir.path(), &report).unwrap();

        assert_eq!(2, report.get_count("restore.file"));
        let dest = &destdir.path();
        assert_that(&dest.join("hello").as_path()).is_a_file();
        assert_that(&dest.join("subdir").as_path()).is_a_directory();
        assert_that(&dest.join("subdir").join("subfile").as_path()).is_a_file();

        // TODO: Test restore empty file.
        // TODO: Test file contents are as expected.
        // TODO: Test restore of larger files.
        // TODO: Test restore of symlinks where supported.
    }
}
