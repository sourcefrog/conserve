// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Bands are the top-level structure inside an archive.
//!
//! Each band contains up to one version of each file, arranged in sorted order within the
//! band.
//!
//! Bands can stack on top of each other to create a tree of incremental backups.


use std::fs;
use std::path::{Path, PathBuf};

use time;

use super::{BandId, Report};
use super::block::BlockDir;
use super::errors::*;
use super::index;
use super::index::IndexBuilder;
use super::io::{file_exists, write_json_uncompressed};

static BLOCK_DIR: &'static str = "d";
static INDEX_DIR: &'static str = "i";
static HEAD_FILENAME: &'static str = "BANDHEAD";
static TAIL_FILENAME: &'static str = "BANDTAIL";

/// All backup data is stored in a band.
#[derive(Debug)]
pub struct Band {
    id: BandId,
    path_buf: PathBuf,
    block_dir_path: PathBuf,
    index_dir_path: PathBuf,
}


#[derive(Debug, RustcDecodable, RustcEncodable)]
struct BandHead {
    start_time: u64,
}


#[derive(Debug, RustcDecodable, RustcEncodable)]
struct BandTail {
    end_time: u64,
}


impl Band {
    /// Make a new band (and its on-disk directory).
    ///
    /// Publicly, prefer Archive::create_band.
    pub fn create(archive_dir: &Path, id: BandId, report: &Report) -> Result<Band> {
        let new = Band::new(archive_dir, id);

        try!(fs::create_dir(&new.path_buf));
        try!(fs::create_dir(&new.block_dir_path));
        try!(fs::create_dir(&new.index_dir_path));
        info!("create band {:?}", &new.path_buf);

        let head = BandHead { start_time: time::get_time().sec as u64 };
        try!(write_json_uncompressed(&new.path_buf.join(HEAD_FILENAME), &head, report));
        Ok(new)
    }

    /// Mark this band closed: no more blocks should be written after this.
    pub fn close(self: &Band, report: &Report) -> Result<()> {
        let tail = BandTail { end_time: time::get_time().sec as u64 };
        write_json_uncompressed(&self.tail_path(), &tail, report)
    }

    pub fn open(archive_dir: &Path, id: &BandId, report: &Report) -> Result<Band> {
        // TODO: Check header file.
        let _ = report;
        Ok(Band::new(archive_dir, id.clone()))
    }

    /// Create a new in-memory Band object.
    ///
    /// Use `create` or `open` to create or open the on-disk directory.
    fn new(archive_dir: &Path, id: BandId) -> Band {
        let mut path_buf = archive_dir.to_path_buf();
        path_buf.push(id.as_string());
        let mut block_dir_path = path_buf.clone();
        block_dir_path.push(BLOCK_DIR);
        let mut index_dir_path = path_buf.clone();
        index_dir_path.push(INDEX_DIR);

        Band {
            id: id,
            path_buf: path_buf,
            block_dir_path: block_dir_path,
            index_dir_path: index_dir_path,
        }
    }

    pub fn is_closed(self: &Band) -> Result<bool> {
        file_exists(&self.tail_path())
    }

    fn tail_path(self: &Band) -> PathBuf {
        self.path_buf.join(TAIL_FILENAME)
    }

    #[allow(unused)]
    pub fn path(self: &Band) -> &Path {
        &self.path_buf
    }

    pub fn block_dir(self: &Band) -> BlockDir {
        BlockDir::new(&self.block_dir_path)
    }

    pub fn index_builder(self: &Band) -> IndexBuilder {
        IndexBuilder::new(&self.index_dir_path)
    }

    /// Make an iterator that will return all entries in this band.
    pub fn index_iter(&self, report: &Report) -> Result<index::Iter> {
        index::read(&self.index_dir_path, report)
    }
}


#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;

    use super::*;
    use super::super::testfixtures::ScratchArchive;
    use super::super::{BandId, Report};

    #[test]
    fn create_band() {
        use super::super::io::list_dir;
        let af = ScratchArchive::new();
        let report = &Report::new();
        let band = Band::create(af.path(), BandId::from_string("b0001").unwrap(), report).unwrap();
        assert!(band.path().to_str().unwrap().ends_with("b0001"));
        assert!(fs::metadata(band.path()).unwrap().is_dir());

        let (file_names, dir_names) = list_dir(band.path()).unwrap();
        assert_eq!(file_names.len(), 1);
        assert_eq!(dir_names.len(), 2);
        assert!(dir_names.contains("d") && dir_names.contains("i"));
        assert!(file_names.contains("BANDHEAD"));
        assert!(!band.is_closed().unwrap());

        band.close(report).unwrap();
        let (file_names, dir_names) = list_dir(band.path()).unwrap();
        assert_eq!(file_names.len(), 2);
        assert_eq!(dir_names.len(), 2);
        assert!(file_names.contains("BANDTAIL"));

        assert!(band.is_closed().unwrap());

        let band_id = BandId::from_string("b0001").unwrap();
        let band2 = Band::open(af.path(), &band_id, report)
            .expect("failed to open band");
        assert!(band2.is_closed().unwrap());
    }

    #[test]
    fn create_existing_band() {
        let af = ScratchArchive::new();
        let band_id = BandId::from_string("b0001").unwrap();
        Band::create(af.path(), band_id.clone(), &Report::new()).unwrap();
        let e = Band::create(af.path(), band_id, &Report::new()).unwrap_err();
        if let ErrorKind::Io(ref ioerror) = *e.kind() {
            assert_eq!(ioerror.kind(), io::ErrorKind::AlreadyExists);
        } else {
            panic!("expected an ioerror, got {:?}", e);
        };
    }
}
