//! Restore from the archive to the filesystem.

use std::fs;
use std::io;
use std::io::prelude::*;
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

pub fn run(archive: PathBuf, destination: PathBuf, report: &Report) -> Result<()> {
    // TODO: Maybe Move this to a method on Restore?
    let archive = try!(Archive::open(&archive));
    let band_id = archive.last_band_id().unwrap().expect("archive is empty");
    let band = Band::open(archive.path(), &band_id, report).unwrap();
    let block_dir = band.block_dir();
    Restore {
        band: band,
        block_dir: block_dir,
        report: report,
        destination: destination
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
        info!("file block addresses are: {:?}", entry.addrs);
        for addr in &entry.addrs {
            let block_vec = try!(self.block_dir.get(&addr, self.report));
            try!(io::copy(&mut block_vec.as_slice(), &mut af));
        }
        af.close(self.report)
    }
}
