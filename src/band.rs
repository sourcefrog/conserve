// Conserve backup system.
// Copyright 2015-2025 Martin Pool.

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

use std::sync::Arc;

use crate::flags::FormatFlags;
use crate::transport::Transport;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tracing::{debug, trace, warn};

use crate::jsonio::{read_json, write_json};
use crate::misc::remove_item;
use crate::monitor::Monitor;
use crate::transport::ListDir;
use crate::*;

static INDEX_DIR: &str = "i";

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
#[derive(Debug, Clone)]
pub struct Band {
    band_id: BandId,

    /// Transport pointing to the archive directory.
    transport: Transport,

    /// Deserialized band head info.
    head: Head,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Head {
    /// Seconds since the Unix epoch when writing of this band began.
    start_time: i64,

    /// Semver string for the minimum Conserve version to read this band
    /// correctly.
    band_format_version: Option<String>,

    /// Format flags that must be understood to read this band and the
    /// referenced data correctly.
    #[serde(default = "FormatFlags::empty")]
    format_flags: FormatFlags,
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
    pub(crate) async fn create(archive: &Archive) -> Result<Band> {
        Band::create_ext(archive, &BackupOptions::default()).await
    }

    pub(crate) async fn create_ext(
        archive: &Archive,
        backup_options: &BackupOptions,
    ) -> Result<Band> {
        let band_id = archive
            .last_band_id()
            .await?
            .map_or_else(BandId::zero, |b| b.next_sibling());
        trace!(?band_id, "Create band");
        let transport = archive.transport().chdir(&band_id.to_string());
        transport.create_dir("").await?;
        transport.create_dir(INDEX_DIR).await?;
        let format_flags = &backup_options.format_flags;
        let band_format_version = if format_flags.is_empty() {
            Some("0.6.3".to_owned())
        } else {
            Some("23.2.0".to_owned())
        };
        let start_time = backup_options
            .override_start_time
            .unwrap_or_else(OffsetDateTime::now_utc)
            .unix_timestamp();
        let head = Head {
            start_time,
            band_format_version,
            format_flags: format_flags.clone(),
        };
        write_json(&transport, BAND_HEAD_FILENAME, &head).await?;
        Ok(Band {
            band_id,
            head,
            transport,
        })
    }

    /// Mark this band closed: no more blocks should be written after this.
    pub async fn close(&self, index_hunk_count: u64, backup_options: &BackupOptions) -> Result<()> {
        // TODO: This should perhaps be on a specific band writer object, and allow it to only be closed once?
        let end_time = backup_options
            .override_start_time
            .unwrap_or_else(OffsetDateTime::now_utc)
            .unix_timestamp();
        write_json(
            &self.transport,
            BAND_TAIL_FILENAME,
            &Tail {
                end_time,
                index_hunk_count: Some(index_hunk_count),
            },
        )
        .await
        .map_err(Error::from)
    }

    /// Open the band with the given id.
    pub async fn open(archive: &Archive, band_id: BandId) -> Result<Band> {
        let transport = archive.transport().chdir(&band_id.to_string());
        let head: Head = read_json(&transport, BAND_HEAD_FILENAME)
            .await?
            .ok_or(Error::BandHeadMissing { band_id })?;
        if let Some(version) = &head.band_format_version {
            if !band_version_supported(version) {
                return Err(Error::UnsupportedBandVersion {
                    band_id,
                    version: version.to_owned(),
                });
            }
        } else {
            debug!("Old(?) band {band_id} has no format version");
            // Unmarked, old bands, are accepted for now. In the next archive
            // version, band version markers ought to become mandatory.
        }

        Ok(Band {
            band_id: band_id.to_owned(),
            head,
            transport,
        })
    }

    /// Delete a band.
    pub async fn delete(archive: &Archive, band_id: BandId) -> Result<()> {
        // TODO: Count how many files were deleted, and the total size?
        archive
            .transport()
            .remove_dir_all(&band_id.to_string())
            .await
            .map_err(|err| {
                if err.is_not_found() {
                    Error::BandNotFound { band_id }
                } else {
                    Error::from(err)
                }
            })
    }

    pub async fn is_closed(&self) -> Result<bool> {
        self.transport
            .is_file(BAND_TAIL_FILENAME)
            .await
            .map_err(Error::from)
    }

    pub fn id(&self) -> BandId {
        self.band_id
    }

    /// Get the minimum supported version for this band.
    pub fn band_format_version(&self) -> Option<&str> {
        self.head.band_format_version.as_deref()
    }

    /// Get the format flags in this band, from [flags].
    pub fn format_flags(&self) -> &FormatFlags {
        &self.head.format_flags
    }

    pub fn index_writer(&self, monitor: Arc<dyn Monitor>) -> IndexWriter {
        IndexWriter::new(self.transport.chdir(INDEX_DIR), monitor)
    }

    /// Get read-only access to the index of this band.
    pub fn index(&self) -> IndexRead {
        IndexRead::open(self.transport.chdir(INDEX_DIR))
    }

    /// Return info about the state of this band.
    pub async fn get_info(&self) -> Result<Info> {
        let tail_option: Option<Tail> = read_json(&self.transport, BAND_TAIL_FILENAME).await?;
        let start_time =
            OffsetDateTime::from_unix_timestamp(self.head.start_time).map_err(|_| {
                Error::InvalidMetadata {
                    details: format!("Invalid band start timestamp {:?}", self.head.start_time),
                }
            })?;
        let end_time = tail_option
            .as_ref()
            .map(|tail| {
                OffsetDateTime::from_unix_timestamp(tail.end_time).map_err(|_| {
                    Error::InvalidMetadata {
                        details: format!("Invalid band end timestamp {:?}", tail.end_time),
                    }
                })
            })
            .transpose()?;
        Ok(Info {
            id: self.band_id,
            is_closed: tail_option.is_some(),
            start_time,
            end_time,
            index_hunk_count: tail_option.as_ref().and_then(|tail| tail.index_hunk_count),
        })
    }

    pub async fn validate(&self, monitor: Arc<dyn Monitor>) -> Result<()> {
        let ListDir { mut files, dirs } = self.transport.list_dir("").await?;
        if !files.contains(&BAND_HEAD_FILENAME.to_string()) {
            monitor.error(Error::BandHeadMissing {
                band_id: self.band_id,
            });
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

    use crate::monitor::test::TestMonitor;

    use super::*;

    #[tokio::test]
    async fn create_and_reopen_band() {
        let backup_options = BackupOptions::default();
        let archive = Archive::create_temp().await;
        let band = Band::create(&archive).await.unwrap();
        let archive_path = archive.transport().local_path().unwrap();

        let band_dir = archive_path.join("b0000");
        assert!(band_dir.is_dir());

        assert!(band_dir.join("BANDHEAD").is_file());
        assert!(!band_dir.join("BANDTAIL").exists());
        assert!(band_dir.join("i").is_dir());

        assert!(!band.is_closed().await.unwrap());

        band.close(0, &backup_options).await.unwrap();
        assert!(band_dir.join("BANDTAIL").is_file());
        assert!(band.is_closed().await.unwrap());

        let band_id = BandId::from_str("b0000").unwrap();
        let band2 = Band::open(&archive, band_id)
            .await
            .expect("failed to re-open band");
        assert!(band2.is_closed().await.unwrap());

        // Try get_info
        let info = band2.get_info().await.expect("get_info failed");
        assert_eq!(info.id.to_string(), "b0000");
        assert!(info.is_closed);
        assert_eq!(info.index_hunk_count, Some(0));
        let dur = info.end_time.expect("info has an end_time") - info.start_time;
        // Test should have taken (much) less than 5s between starting and finishing
        // the band.  (It might fail if you set a breakpoint right there.)
        assert!(dur < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn delete_band() {
        let archive = Archive::create_temp().await;
        let _band = Band::create(&archive).await.unwrap();
        assert!(archive.transport().is_file("b0000/BANDHEAD").await.unwrap());

        Band::delete(&archive, BandId::new(&[0]))
            .await
            .expect("delete band");

        assert!(!archive.transport().is_file("b0000").await.unwrap());
        assert!(!archive.transport().is_file("b0000/BANDHEAD").await.unwrap());
    }

    #[tokio::test]
    async fn unsupported_band_version() {
        let archive = Archive::create_temp().await;
        let path = archive.transport().local_path().unwrap();
        fs::create_dir(path.join("b0000")).unwrap();
        let head = json!({
            "start_time": 0,
            "band_format_version": "8888.8.8",
        });
        fs::write(
            path.join("b0000").join(BAND_HEAD_FILENAME),
            head.to_string(),
        )
        .unwrap();

        let e = Band::open(&archive, BandId::zero()).await;
        let e_str = e.unwrap_err().to_string();
        assert!(
            e_str.contains("Unsupported band version \"8888.8.8\" in b0000"),
            "bad band version: {e_str:#?}"
        );
    }

    #[tokio::test]
    async fn create_bands() {
        let archive = Archive::create_temp().await;
        assert!(archive.transport().local_path().unwrap().join("d").is_dir());

        // Make one band
        let _band1 = Band::create(&archive).await.unwrap();
        let band_path = archive.transport().local_path().unwrap().join("b0000");
        assert!(band_path.is_dir());
        assert!(band_path.join("BANDHEAD").is_file());
        assert!(band_path.join("i").is_dir());

        assert_eq!(
            archive.list_band_ids().await.unwrap(),
            vec![BandId::new(&[0])]
        );
        assert_eq!(
            archive.last_band_id().await.unwrap(),
            Some(BandId::new(&[0]))
        );

        // Try creating a second band.
        let _band2 = Band::create(&archive).await.unwrap();
        assert_eq!(
            archive.list_band_ids().await.unwrap(),
            vec![BandId::new(&[0]), BandId::new(&[1])]
        );
        assert_eq!(
            archive.last_band_id().await.unwrap(),
            Some(BandId::new(&[1]))
        );

        assert_eq!(
            archive
                .referenced_blocks(&archive.list_band_ids().await.unwrap(), TestMonitor::arc())
                .await
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            archive.all_blocks(TestMonitor::arc()).await.unwrap().len(),
            0
        );
    }

    #[tokio::test]
    async fn default_format_flags_are_empty() {
        let archive = Archive::create_temp().await;

        let orig_band = Band::create(&archive).await.unwrap();
        let flags = orig_band.format_flags();
        assert!(flags.is_empty(), "{flags:?}");

        let band = Band::open(&archive, orig_band.id()).await.unwrap();
        println!("{band:?}");
        assert!(band.format_flags().is_empty());

        assert_eq!(band.band_format_version(), Some("0.6.3"));
        // TODO: When we do support some flags, check that the minimum version is 23.2.
    }
}
