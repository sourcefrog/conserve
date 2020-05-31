// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

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

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;

use crate::io::file_exists;
use crate::jsonio;
use crate::misc::remove_item;
use crate::*;

static INDEX_DIR: &str = "i";
static HEAD_FILENAME: &str = "BANDHEAD";
static TAIL_FILENAME: &str = "BANDTAIL";

/// Band format-compatibility. Bands written out by this program, can only be
/// read correctly by versions equal or later than the stated version.
pub const BAND_FORMAT_VERSION: &str = "0.6.3";

fn band_version_requirement() -> semver::VersionReq {
    semver::VersionReq::parse("<=0.6.3").unwrap()
}

fn band_version_supported(version: &str) -> bool {
    semver::Version::parse(&version)
        .map(|sv| band_version_requirement().matches(&sv))
        .unwrap_or(false)
}

/// All backup data is stored in a band.
#[derive(Debug)]
pub struct Band {
    id: BandId,
    path_buf: PathBuf,
    pub index_dir_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Head {
    /// Seconds since the Unix epoch when writing of this band began.
    start_time: i64,

    /// Semver string for the minimum Conserve version to read this band
    /// correctly.
    band_format_version: Option<String>,
}

/// Format of the on-disk tail file.
#[derive(Debug, Serialize, Deserialize)]
struct Tail {
    end_time: i64,
}

/// Readonly summary info about a band, from `Band::get_info`.
pub struct Info {
    pub id: BandId,
    pub is_closed: bool,

    /// Time Conserve started writing this band.
    pub start_time: DateTime<Utc>,

    /// Time this band was completed, if it is complete.
    pub end_time: Option<DateTime<Utc>>,
}

// TODO: Maybe merge this with StoredTree? The distinction seems small.
impl Band {
    /// Make a new band (and its on-disk directory).
    ///
    /// The Band gets the next id after those that already exist.
    pub fn create(archive: &Archive) -> Result<Band> {
        let new_band_id = archive
            .last_band_id()?
            .map_or_else(BandId::zero, |b| b.next_sibling());
        let archive_dir = archive.path();
        let new = Band::new(archive_dir, new_band_id);
        fs::create_dir(&new.path_buf).context(errors::CreateBand)?;
        fs::create_dir(&new.index_dir_path).context(errors::CreateBand)?;
        let head = Head {
            start_time: Utc::now().timestamp(),
            band_format_version: Some(BAND_FORMAT_VERSION.to_owned()),
        };
        jsonio::write_json_metadata_file(&new.head_path(), &head)?;
        Ok(new)
    }

    /// Mark this band closed: no more blocks should be written after this.
    pub fn close(&self) -> Result<()> {
        let tail = Tail {
            end_time: Utc::now().timestamp(),
        };
        jsonio::write_json_metadata_file(&self.tail_path(), &tail)
    }

    /// Open the band with the given id.
    pub fn open(archive: &Archive, band_id: &BandId) -> Result<Band> {
        let new = Band::new(archive.path(), band_id.clone());
        let head = new.read_head()?;
        if let Some(version) = head.band_format_version {
            if !band_version_supported(&version) {
                return Err(Error::UnsupportedBandVersion {
                    path: new.path().into(),
                    version,
                });
            }
        } else {
            // Unmarked, old bands, are accepted for now. In the next archive
            // version, band version markers ought to become mandatory.
        }
        Ok(new)
    }

    /// Create a new in-memory Band object.
    ///
    /// Instead of creating the in-memory object you typically should either
    /// `create` or `open` the band corresponding to in-archive directory.
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
        let path = self.tail_path();
        file_exists(&path).context(errors::ReadMetadata { path })
    }

    pub fn path(&self) -> &Path {
        &self.path_buf
    }

    pub fn id(&self) -> &BandId {
        &self.id
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

    /// Get read-only access to the index of this band.
    pub fn index(&self) -> ReadIndex {
        ReadIndex::new(&self.index_dir_path)
    }

    /// Return an iterator through entries in this band.
    pub fn iter_entries(&self) -> Result<index::IndexEntryIter> {
        index::IndexEntryIter::open(&self.index_dir_path)
    }

    fn read_head(&self) -> Result<Head> {
        jsonio::read_json_metadata_file(&self.head_path())
    }

    fn read_tail(&self) -> Result<Tail> {
        jsonio::read_json_metadata_file(&self.tail_path())
    }

    /// Return info about the state of this band.
    pub fn get_info(&self) -> Result<Info> {
        let head = self.read_head()?;
        let is_closed = self.is_closed()?;
        let end_time = if is_closed {
            Some(Utc.timestamp(self.read_tail()?.end_time, 0))
        } else {
            None
        };
        Ok(Info {
            id: self.id.clone(),
            is_closed,
            start_time: Utc.timestamp(head.start_time, 0),
            end_time,
        })
    }

    pub fn validate(&self) -> Result<()> {
        let (mut files, dirs) =
            list_dir(self.path()).context(errors::ReadMetadata { path: self.path() })?;
        if !files.contains(&HEAD_FILENAME.to_string()) {
            ui::problem(&format!("No band head file in {:?}", self.path()));
        }
        remove_item(&mut files, &HEAD_FILENAME);
        remove_item(&mut files, &TAIL_FILENAME);
        if !files.is_empty() {
            ui::problem(&format!(
                "Unexpected files in {:?}: {:?}",
                self.path(),
                files
            ));
        }

        if dirs != [INDEX_DIR.to_string()] {
            ui::problem(&format!(
                "Incongruous directories in {:?}: {:?}",
                self.path(),
                dirs
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::Duration;
    use serde_json::json;

    use super::*;
    use crate::test_fixtures::ScratchArchive;

    #[test]
    fn create_and_reopen_band() {
        let af = ScratchArchive::new();
        let band = Band::create(&af).unwrap();
        assert!(band.path().to_str().unwrap().ends_with("b0000"));
        assert!(fs::metadata(band.path()).unwrap().is_dir());

        let (file_names, dir_names) = list_dir(band.path()).unwrap();
        assert_eq!(file_names, &["BANDHEAD"]);
        assert_eq!(dir_names, ["i"]);
        assert!(!band.is_closed().unwrap());

        band.close().unwrap();
        let (file_names, dir_names) = list_dir(band.path()).unwrap();
        assert_eq!(file_names, &["BANDHEAD", "BANDTAIL"]);
        assert_eq!(dir_names, ["i"]);
        assert!(band.is_closed().unwrap());

        let band_id = BandId::from_string("b0000").unwrap();
        let band2 = Band::open(&af, &band_id).expect("failed to open band");
        assert!(band2.is_closed().unwrap());

        // Try get_info
        let info = band2.get_info().expect("get_info failed");
        assert_eq!(info.id.to_string(), "b0000");
        assert_eq!(info.is_closed, true);
        let dur = info.end_time.expect("info has an end_time") - info.start_time;
        // Test should have taken (much) less than 5s between starting and finishing
        // the band.  (It might fail if you set a breakpoint right there.)
        assert!(dur < Duration::seconds(5));
    }

    #[test]
    fn unsupported_band_version() {
        let af = ScratchArchive::new();
        fs::create_dir(af.path().join("b0000")).unwrap();
        let head = json!({
            "start_time": 0,
            "band_format_version": "0.8.8",
        });
        fs::write(
            af.path().join("b0000").join(HEAD_FILENAME),
            head.to_string(),
        )
        .unwrap();

        let e = Band::open(&af, &BandId::zero());
        assert!(e.is_err());
        let e_str = e.unwrap_err().to_string();
        assert!(e_str.contains("Band version \"0.8.8\" in"), e_str);
    }
}
