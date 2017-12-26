// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! Bands are the top-level structure inside an archive.
//!
//! Each band contains up to one version of each file, arranged in sorted order within the
//! band.
//!
//! Bands can stack on top of each other to create a tree of incremental backups.
//!
//! To read a consistent tree possibly composed from several incremental backups, use
//! StoredTree rather than the Band itself.


use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, UTC};

use super::*;
use super::index;
use super::jsonio;
use super::io::file_exists;

use globset::GlobSet;

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
struct Head {
    start_time: i64,
}


/// Format of the on-disk tail file.
#[derive(Debug, RustcDecodable, RustcEncodable)]
struct Tail {
    end_time: i64,
}


/// Readonly summary info about a band, from `Band::get_info`.
pub struct Info {
    pub id: BandId,
    pub is_closed: bool,

    /// Time Conserve started writing this band.
    pub start_time: DateTime<UTC>,

    /// Time this band was completed, if it is complete.
    pub end_time: Option<DateTime<UTC>>,
}


impl Band {
    /// Make a new band (and its on-disk directory).
    ///
    /// Prefer Archive::create_band.
    pub(crate) fn create(archive_dir: &Path, id: BandId, report: &Report) -> Result<Band> {
        let new = Band::new(archive_dir, id);

        fs::create_dir(&new.path_buf)?;
        fs::create_dir(&new.block_dir_path)?;
        fs::create_dir(&new.index_dir_path)?;
        info!("Created band {} in {:?}", new.id, &archive_dir);

        let head = Head { start_time: UTC::now().timestamp() };
        jsonio::write(&new.head_path(), &head, report)?;
        Ok(new)
    }

    /// Mark this band closed: no more blocks should be written after this.
    pub fn close(self: &Band, report: &Report) -> Result<()> {
        let tail = Tail { end_time: UTC::now().timestamp() };
        jsonio::write(&self.tail_path(), &tail, report)
    }

    /// Open a given band. Prefer `Archive.open_band`.
    pub(crate) fn open(archive_dir: &Path, id: &BandId, report: &Report) -> Result<Band> {
        let new = Band::new(archive_dir, id.clone());
        new.read_head(&report)?; // Just check it can be read
        Ok(new)
    }

    /// Create a new in-memory Band object.
    ///
    /// Prefer `Archive.create_band`.
    pub(crate) fn new(archive_dir: &Path, id: BandId) -> Band {
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

    pub fn path(self: &Band) -> &Path {
        &self.path_buf
    }

    pub fn id(self: &Band) -> BandId {
        self.id.clone()
    }

    fn head_path(&self) -> PathBuf {
        self.path_buf.join(HEAD_FILENAME)
    }

    fn tail_path(self: &Band) -> PathBuf {
        self.path_buf.join(TAIL_FILENAME)
    }

    pub fn block_dir(self: &Band) -> BlockDir {
        BlockDir::new(&self.block_dir_path)
    }

    pub fn index_builder(self: &Band) -> IndexBuilder {
        IndexBuilder::new(&self.index_dir_path)
    }

    /// Make an iterator that will return all entries in this band.
    pub fn index_iter(&self, excludes: &GlobSet, report: &Report) -> Result<index::Iter> {
        index::read(&self.index_dir_path, excludes, report)
    }

    fn read_head(&self, report: &Report) -> Result<Head> {
        jsonio::read(&self.head_path(), &report)
    }

    fn read_tail(&self, report: &Report) -> Result<Tail> {
        jsonio::read(&self.tail_path(), &report)
    }

    /// Return info about the state of this band.
    pub fn get_info(&self, report: &Report) -> Result<Info> {
        let head = self.read_head(&report)?;
        let is_closed = self.is_closed()?;
        let end_time = if is_closed {
            Some(UTC.timestamp(self.read_tail(&report)?.end_time, 0))
        } else {
            None
        };
        Ok(Info {
            id: self.id.clone(),
            is_closed: is_closed,
            start_time: UTC.timestamp(head.start_time, 0),
            end_time: end_time,
        })
    }
}


#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;

    use chrono::Duration;

    use super::super::*;
    use test_fixtures::ScratchArchive;

    #[test]
    fn create_and_reopen_band() {
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
        let band2 = Band::open(af.path(), &band_id, report).expect("failed to open band");
        assert!(band2.is_closed().unwrap());

        // Try get_info
        let info = band2.get_info(&Report::new()).expect("get_info failed");
        assert_eq!(info.id.as_string(), "b0001");
        assert_eq!(info.is_closed, true);
        let dur = info.end_time.expect("info has an end_time") - info.start_time;
        // Test should have taken (much) less than 5s between starting and finishing
        // the band.  (It might fail if you set a breakpoint right there.)
        assert!(dur < Duration::seconds(5));
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
