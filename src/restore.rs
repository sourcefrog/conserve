use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::*;
use super::apath;
use super::errors::*;
use super::index;

pub struct Restore<'a> {
    pub band: Band,
    pub report: &'a Report,
    pub destination: PathBuf,
}

impl<'a> Restore<'a> {
    pub fn run(&mut self) -> Result<()> {
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
}
