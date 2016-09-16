use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::*;
use super::apath;
use super::errors::*;
use super::index;

pub struct Restore<'a> {
    pub band: Band,
    pub report: &'a mut Report,
    pub destination: PathBuf,
}

impl<'a> Restore<'a> {
    pub fn run(&mut self) -> Result<()> {
        let mut iter = try!(self.band.index_iter());
        for entry in iter.by_ref() {
            let entry = try!(entry);
            // TODO: Continue even if one fails
            try!(self.restore_one(&entry));
        }
        self.report.merge_from(&iter.report);
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

    fn restore_dir(&self, _entry: &index::Entry, dest: &Path) -> Result<()> {
        match fs::create_dir(dest) {
            Ok(_) => Ok(()),
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
