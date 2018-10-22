// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

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

use super::index;
use super::io::file_exists;
use super::jsonio;
use super::*;

use globset::GlobSet;

static INDEX_DIR: &'static str = "i";
static HEAD_FILENAME: &'static str = "BANDHEAD";
static TAIL_FILENAME: &'static str = "BANDTAIL";

/// All backup data is stored in a band.
#[derive(Debug)]
pub struct Band {
    id: BandId,
    path_buf: PathBuf,
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
    /// The Band gets the next id after those that already exist.
    pub fn create(archive: &Archive) -> Result<Band> {
        let new_band_id = match archive.last_band_id() {
            Err(Error(ErrorKind::ArchiveEmpty, _)) => BandId::zero(),
            Ok(b) => b.next_sibling(),
            Err(e) => return Err(e),
        };
        Band::create_specific_id(archive, new_band_id)
    }

    /// Create a Band with a given id.
    fn create_specific_id(archive: &Archive, id: BandId) -> Result<Band> {
        let archive_dir = archive.path();
        let new = Band::new(archive_dir, id);

        fs::create_dir(&new.path_buf)?;
        fs::create_dir(&new.index_dir_path)?;

        let head = Head {
            start_time: UTC::now().timestamp(),
        };
        jsonio::write(&new.head_path(), &head, archive.report())?;
        Ok(new)
    }

    /// Mark this band closed: no more blocks should be written after this.
    pub fn close(&self, report: &Report) -> Result<()> {
        let tail = Tail {
            end_time: UTC::now().timestamp(),
        };
        jsonio::write(&self.tail_path(), &tail, report)
    }

    /// Open a given band, or by default the latest complete backup in the archive.
    pub fn open(archive: &Archive, band_id: &BandId) -> Result<Band> {
        let new = Band::new(archive.path(), band_id.clone());
        new.read_head(&archive.report())?; // Just check it can be read
        Ok(new)
    }

    /// Create a new in-memory Band object.
    ///
    /// Instead of creating the in-memory object you typically should either `create` or `open` the
    /// band corresponding to in-archive directory.
    fn new(archive_dir: &Path, id: BandId) -> Band {
        let mut path_buf = archive_dir.to_path_buf();
        path_buf.push(id.to_string());
        let mut index_dir_path = path_buf.clone();
        index_dir_path.push(INDEX_DIR);
        Band {
            id,
            path_buf,
            index_dir_path,
        }
    }

    pub fn is_closed(&self) -> Result<bool> {
        file_exists(&self.tail_path())
    }

    pub fn path(&self) -> &Path {
        &self.path_buf
    }

    pub fn id(&self) -> BandId {
        self.id.clone()
    }

    fn head_path(&self) -> PathBuf {
        self.path_buf.join(HEAD_FILENAME)
    }

    fn tail_path(&self) -> PathBuf {
        self.path_buf.join(TAIL_FILENAME)
    }

    pub fn index_builder(&self) -> IndexBuilder {
        IndexBuilder::new(&self.index_dir_path)
    }

    /// Make an iterator that will return all entries in this band.
    pub fn index_iter(&self, excludes: &GlobSet, report: &Report) -> Result<index::Iter> {
        index::Iter::open(&self.index_dir_path, excludes, report)
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
            is_closed,
            start_time: UTC.timestamp(head.start_time, 0),
            end_time,
        })
    }

    /// Get the total size in bytes of files stored for this band.
    pub fn get_disk_size(&self) -> Result<u64> {
        // TODO: Better handling than panic of unexpected errors.
        let mut total = 0u64;
        for entry in walkdir::WalkDir::new(self.path()) {
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_file() {
                        total += entry.metadata().unwrap().len();
                    }
                }
                Err(e) => return Err(e.into_io_error().unwrap().into()),
            }
        }
        Ok(total)
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
        let af = ScratchArchive::new();
        let report = &Report::new();
        let band = Band::create(&af).unwrap();
        assert!(band.path().to_str().unwrap().ends_with("b0000"));
        assert!(fs::metadata(band.path()).unwrap().is_dir());

        let (file_names, dir_names) = list_dir(band.path()).unwrap();
        assert_eq!(file_names, &["BANDHEAD"]);
        assert_eq!(dir_names, ["i"]);
        assert!(!band.is_closed().unwrap());

        band.close(report).unwrap();
        let (file_names, dir_names) = list_dir(band.path()).unwrap();
        assert_eq!(file_names, &["BANDHEAD", "BANDTAIL"]);
        assert_eq!(dir_names, ["i"]);
        assert!(band.is_closed().unwrap());

        let band_id = BandId::from_string("b0000").unwrap();
        let band2 = Band::open(&af, &band_id).expect("failed to open band");
        assert!(band2.is_closed().unwrap());

        // Try get_info
        let info = band2.get_info(&Report::new()).expect("get_info failed");
        assert_eq!(info.id.to_string(), "b0000");
        assert_eq!(info.is_closed, true);
        let dur = info.end_time.expect("info has an end_time") - info.start_time;
        // Test should have taken (much) less than 5s between starting and finishing
        // the band.  (It might fail if you set a breakpoint right there.)
        assert!(dur < Duration::seconds(5));

        // It takes some amount of space
        let bytes = band2.get_disk_size().unwrap();
        assert!(bytes > 10 && bytes < 8000, bytes);
    }

    #[test]
    fn create_existing_band() {
        let af = ScratchArchive::new();
        let band_id = BandId::from_string("b0001").unwrap();
        Band::create_specific_id(&af, band_id.clone()).unwrap();
        let e = Band::create_specific_id(&af, band_id).unwrap_err();
        if let ErrorKind::Io(ref ioerror) = *e.kind() {
            assert_eq!(ioerror.kind(), io::ErrorKind::AlreadyExists);
        } else {
            panic!("expected an ioerror, got {:?}", e);
        };
    }
}
