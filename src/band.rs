// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Bands are the top-level structure inside an archive.
//!
//! Each band contains up to one version of each file, arranged in sorted order within the
//! band.
//!
//! Bands can stack on top of each other to create a tree of incremental backups.
//!
//! To read a consistent tree possibly composed from several incremental backups, use
//! StoredTree rather than the Band itself.

use std::borrow::Cow;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tracing::{debug, error, warn};

use crate::jsonio::{read_json, write_json};
use crate::misc::remove_item;
use crate::transport::{ListDirNames, Transport};
use crate::*;

static INDEX_DIR: &str = "i";

/// Per-band format flags.
pub mod flags {
    use std::borrow::Cow;

    /// Default flags for newly created bands.
    pub static DEFAULT: &[Cow<'static, str>] = &[];

    /// All the flags understood by this version of Conserve.
    pub static SUPPORTED: &[&'static str] = &[];
}

/// Describes how to select a band from an archive.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BandSelectionPolicy {
    /// Open the latest complete band.
    LatestClosed,
    /// Open the latest band, regardless of whether it's complete.
    Latest,
    /// Open the band with the specified id.
    Specified(BandId),
}

fn band_version_requirement() -> semver::VersionReq {
    semver::VersionReq::parse(&format!("<={}", crate::VERSION)).unwrap()
}

fn band_version_supported(version: &str) -> bool {
    semver::Version::parse(version)
        .map(|sv| band_version_requirement().matches(&sv))
        .unwrap()
}

/// Each backup makes a new `band` containing an index directory.
#[derive(Debug)]
pub struct Band {
    band_id: BandId,

    /// Transport pointing to the archive directory.
    transport: Box<dyn Transport>,

    /// Deserialized band head info.
    head: Head,
}

#[derive(Debug, Serialize, Deserialize)]
struct Head {
    /// Seconds since the Unix epoch when writing of this band began.
    start_time: i64,

    /// Semver string for the minimum Conserve version to read this band
    /// correctly.
    band_format_version: Option<String>,

    /// Format flags that must be understood to read this band and the
    /// referenced data correctly.
    #[serde(default)]
    format_flags: Vec<Cow<'static, str>>,
}

/// Format of the on-disk tail file.
#[derive(Debug, Serialize, Deserialize)]
struct Tail {
    /// Seconds since the Unix epoch when the band was closed.
    end_time: i64,

    /// Number of index hunks in this band, to enable validation that none are missing.
    ///
    /// Present from 0.6.4 onwards.
    index_hunk_count: Option<u64>,
}

/// Readonly summary info about a band, from `Band::get_info`.
pub struct Info {
    pub id: BandId,
    pub is_closed: bool,

    /// Time Conserve started writing this band.
    pub start_time: OffsetDateTime,

    /// Time this band was completed, if it is complete.
    pub end_time: Option<OffsetDateTime>,

    /// Number of hunks present in the index, if that is known.
    pub index_hunk_count: Option<u64>,
}

// TODO: Maybe merge Band with StoredTree and/or with the Index classes? The distinction seems
// small.
impl Band {
    /// Make a new band (and its on-disk directory).
    ///
    /// The Band gets the next id after those that already exist.
    pub fn create(archive: &Archive) -> Result<Band> {
        Band::create_with_flags(archive, flags::DEFAULT)
    }

    pub fn create_with_flags(
        archive: &Archive,
        format_flags: &[Cow<'static, str>],
    ) -> Result<Band> {
        format_flags
            .iter()
            .for_each(|f| assert!(flags::SUPPORTED.contains(&f.as_ref()), "unknown flag {f:?}"));
        let band_id = archive
            .last_band_id()?
            .map_or_else(BandId::zero, |b| b.next_sibling());
        let transport: Box<dyn Transport> = archive.transport().sub_transport(&band_id.to_string());
        transport
            .create_dir("")
            .and_then(|()| transport.create_dir(INDEX_DIR))
            .map_err(|source| Error::CreateBand { source })?;
        let band_format_version = if format_flags.is_empty() {
            Some("0.6.3".to_owned())
        } else {
            Some("23.2.0".to_owned())
        };
        let head = Head {
            start_time: OffsetDateTime::now_utc().unix_timestamp(),
            band_format_version,
            format_flags: format_flags.into(),
        };
        write_json(&transport, BAND_HEAD_FILENAME, &head)?;
        Ok(Band {
            band_id,
            head,
            transport,
        })
    }

    /// Mark this band closed: no more blocks should be written after this.
    pub fn close(&self, index_hunk_count: u64) -> Result<()> {
        write_json(
            &self.transport,
            BAND_TAIL_FILENAME,
            &Tail {
                end_time: OffsetDateTime::now_utc().unix_timestamp(),
                index_hunk_count: Some(index_hunk_count),
            },
        )
    }

    /// Open the band with the given id.
    pub fn open(archive: &Archive, band_id: &BandId) -> Result<Band> {
        let transport: Box<dyn Transport> = archive.transport().sub_transport(&band_id.to_string());
        let head: Head = read_json(&transport, BAND_HEAD_FILENAME)?;
        if let Some(version) = &head.band_format_version {
            if !band_version_supported(version) {
                return Err(Error::UnsupportedBandVersion {
                    band_id: band_id.to_owned(),
                    version: version.to_owned(),
                });
            }
        } else {
            debug!("Old(?) band {band_id} has no format version");
            // Unmarked, old bands, are accepted for now. In the next archive
            // version, band version markers ought to become mandatory.
        }

        let unsupported_flags = head
            .format_flags
            .iter()
            .filter(|f| !flags::SUPPORTED.contains(&f.as_ref()))
            .cloned()
            .collect_vec();
        if !unsupported_flags.is_empty() {
            return Err(Error::UnsupportedBandFormatFlags {
                band_id: band_id.clone(),
                unsupported_flags,
            });
        }

        Ok(Band {
            band_id: band_id.to_owned(),
            head,
            transport,
        })
    }

    /// Delete a band.
    pub fn delete(archive: &Archive, band_id: &BandId) -> Result<()> {
        // TODO: Count how many files were deleted, and the total size?
        archive
            .transport()
            .remove_dir_all(&band_id.to_string())
            .map_err(|source| Error::BandDeletion {
                band_id: band_id.clone(),
                source,
            })
    }

    pub fn is_closed(&self) -> Result<bool> {
        self.transport
            .is_file(BAND_TAIL_FILENAME)
            .map_err(Error::from)
    }

    pub fn id(&self) -> &BandId {
        &self.band_id
    }

    /// Get the minimum supported version for this band.
    pub fn band_format_version(&self) -> Option<&str> {
        self.head.band_format_version.as_deref()
    }

    /// Get the format flags in this band, from [flags].
    pub fn format_flags(&self) -> &[Cow<'static, str>] {
        &self.head.format_flags
    }

    pub fn index_builder(&self) -> IndexWriter {
        IndexWriter::new(self.transport.sub_transport(INDEX_DIR))
    }

    /// Get read-only access to the index of this band.
    pub fn index(&self) -> IndexRead {
        IndexRead::open(self.transport.sub_transport(INDEX_DIR))
    }

    /// Return info about the state of this band.
    pub fn get_info(&self) -> Result<Info> {
        let tail_option: Option<Tail> = match read_json(&self.transport, BAND_TAIL_FILENAME) {
            Ok(tail) => Some(tail),
            Err(Error::MetadataNotFound { .. }) => None,
            Err(err) => return Err(err),
        };
        let start_time = OffsetDateTime::from_unix_timestamp(self.head.start_time)
            .expect("invalid band start timestamp");
        let end_time = tail_option.as_ref().map(|tail| {
            OffsetDateTime::from_unix_timestamp(tail.end_time).expect("invalid end timestamp")
        });
        Ok(Info {
            id: self.band_id.clone(),
            is_closed: tail_option.is_some(),
            start_time,
            end_time,
            index_hunk_count: tail_option.as_ref().and_then(|tail| tail.index_hunk_count),
        })
    }

    pub fn validate(&self) -> Result<()> {
        let ListDirNames { mut files, dirs } =
            self.transport.list_dir_names("").map_err(Error::from)?;
        if !files.contains(&BAND_HEAD_FILENAME.to_string()) {
            error!(band_id = ?self.band_id, "Band head file missing");
        }
        remove_item(&mut files, &BAND_HEAD_FILENAME);
        remove_item(&mut files, &BAND_TAIL_FILENAME);
        for unexpected in files {
            warn!(path = ?unexpected, "Unexpected file in band directory");
        }
        for unexpected in dirs.iter().filter(|n| n != &INDEX_DIR) {
            warn!(path = ?unexpected, "Unexpected subdirectory in band directory");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::str::FromStr;
    use std::time::Duration;

    use serde_json::json;

    use crate::test_fixtures::ScratchArchive;

    use super::*;

    #[test]
    fn create_and_reopen_band() {
        let af = ScratchArchive::new();
        let band = Band::create(&af).unwrap();

        let band_dir = af.path().join("b0000");
        assert!(band_dir.is_dir());

        assert!(band_dir.join("BANDHEAD").is_file());
        assert!(!band_dir.join("BANDTAIL").exists());
        assert!(band_dir.join("i").is_dir());

        assert!(!band.is_closed().unwrap());

        band.close(0).unwrap();
        assert!(band_dir.join("BANDTAIL").is_file());
        assert!(band.is_closed().unwrap());

        let band_id = BandId::from_str("b0000").unwrap();
        let band2 = Band::open(&af, &band_id).expect("failed to re-open band");
        assert!(band2.is_closed().unwrap());

        // Try get_info
        let info = band2.get_info().expect("get_info failed");
        assert_eq!(info.id.to_string(), "b0000");
        assert!(info.is_closed);
        assert_eq!(info.index_hunk_count, Some(0));
        let dur = info.end_time.expect("info has an end_time") - info.start_time;
        // Test should have taken (much) less than 5s between starting and finishing
        // the band.  (It might fail if you set a breakpoint right there.)
        assert!(dur < Duration::from_secs(5));
    }

    #[test]
    fn delete_band() {
        let af = ScratchArchive::new();
        let _band = Band::create(&af).unwrap();
        Band::delete(&af, &BandId::new(&[0])).expect("delete band");

        assert!(!af.transport().is_file("b0000").unwrap());
        assert!(!af.transport().is_dir("b0000").unwrap());
    }

    #[test]
    fn unsupported_band_version() {
        let af = ScratchArchive::new();
        fs::create_dir(af.path().join("b0000")).unwrap();
        let head = json!({
            "start_time": 0,
            "band_format_version": "8888.8.8",
        });
        fs::write(
            af.path().join("b0000").join(BAND_HEAD_FILENAME),
            head.to_string(),
        )
        .unwrap();

        let e = Band::open(&af, &BandId::zero());
        let e_str = e.unwrap_err().to_string();
        assert!(
            e_str.contains("Band version \"8888.8.8\" in"),
            "bad band version: {e_str:#?}"
        );
    }
}
