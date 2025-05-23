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

//! Archives holding backup material.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use std::time::Instant;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::blockdir::BlockDir;
use crate::index::stitch::Stitch;
use crate::jsonio::{read_json, write_json};
use crate::monitor::Monitor;
use crate::transport::Transport;
use crate::*;

const HEADER_FILENAME: &str = "CONSERVE";
static BLOCK_DIR: &str = "d";

/// An archive holding backup material.
#[derive(Clone, Debug)]
pub struct Archive {
    /// Holds body content for all file versions.
    pub(crate) block_dir: Arc<BlockDir>,

    /// Transport to the root directory of the archive.
    transport: Transport,
}

#[derive(Debug, Serialize, Deserialize)]
struct ArchiveHeader {
    conserve_archive_version: String,
}

#[derive(Default, Debug)]
pub struct DeleteOptions {
    pub dry_run: bool,
    pub break_lock: bool,
}

impl Archive {
    /// Make a new archive in a local directory.
    pub async fn create_path(path: &Path) -> Result<Archive> {
        Archive::create(Transport::local(path)).await
    }

    /// Make a new archive in a temp directory.
    ///
    /// Panic if the tempdir can't be created.
    pub async fn create_temp() -> Archive {
        Archive::create(Transport::temp()).await.unwrap()
    }

    /// Make a new archive in a new directory accessed by a Transport.
    pub async fn create(transport: Transport) -> Result<Archive> {
        transport.create_dir("").await?;
        let names = transport.list_dir("").await?;
        if !names.files.is_empty() || !names.dirs.is_empty() {
            return Err(Error::NewArchiveDirectoryNotEmpty);
        }
        let block_dir = Arc::new(BlockDir::create(transport.chdir(BLOCK_DIR)).await?);
        let header = ArchiveHeader {
            conserve_archive_version: String::from(ARCHIVE_VERSION),
        };
        write_json(&transport, HEADER_FILENAME, &header).await?;
        Ok(Archive {
            block_dir,
            transport,
        })
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub async fn open_path(path: &Path) -> Result<Archive> {
        Archive::open(Transport::local(path)).await
    }

    pub async fn open(transport: Transport) -> Result<Archive> {
        let header: ArchiveHeader = read_json(&transport, HEADER_FILENAME)
            .await?
            .ok_or(Error::NotAnArchive)?;
        if header.conserve_archive_version != ARCHIVE_VERSION {
            return Err(Error::UnsupportedArchiveVersion {
                version: header.conserve_archive_version,
            });
        }
        let block_dir = Arc::new(BlockDir::open(transport.chdir(BLOCK_DIR)));
        debug!(?header, "Opened archive");
        Ok(Archive {
            block_dir,
            transport,
        })
    }

    /// Return an unsorted list of all blocks in the archive.
    pub async fn all_blocks(&self, monitor: Arc<dyn Monitor>) -> Result<Vec<BlockHash>> {
        self.block_dir.blocks_async(monitor).await
    }

    pub async fn band_exists(&self, band_id: BandId) -> Result<bool> {
        self.transport()
            .is_file(&format!("{}/{}", band_id, crate::BAND_HEAD_FILENAME))
            .await
            .map_err(Error::from)
    }

    pub async fn band_is_closed(&self, band_id: BandId) -> Result<bool> {
        self.transport
            .is_file(&format!("{}/{}", band_id, crate::BAND_TAIL_FILENAME))
            .await
            .map_err(Error::from)
    }

    /// Return an iterator of entries in a selected version.
    // TODO: Maybe drop this and have callers walk the tree themselves?
    pub async fn iter_entries(
        &self,
        band_selection: BandSelectionPolicy,
        subtree: Apath,
        exclude: Exclude,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Stitch> {
        Ok(self
            .open_stored_tree(band_selection)
            .await?
            .iter_entries(subtree, exclude, monitor))
    }

    /// Returns a vector of band ids, in sorted order from first to last.
    pub async fn list_band_ids(&self) -> Result<Vec<BandId>> {
        Ok(self
            .transport
            .list_dir("")
            .await?
            .dirs
            .into_iter()
            .filter(|dir_name| dir_name != BLOCK_DIR)
            .filter_map(|dir_name| dir_name.parse().ok())
            .sorted()
            .collect())
    }

    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    pub async fn resolve_band_id(&self, band_selection: BandSelectionPolicy) -> Result<BandId> {
        match band_selection {
            BandSelectionPolicy::LatestClosed => self
                .last_complete_band()
                .await?
                .map(|band| band.id())
                .ok_or(Error::NoCompleteBands),
            BandSelectionPolicy::Specified(band_id) => Ok(band_id),
            BandSelectionPolicy::Latest => self.last_band_id().await?.ok_or(Error::ArchiveEmpty),
        }
    }

    pub async fn open_stored_tree(
        &self,
        band_selection: BandSelectionPolicy,
    ) -> Result<StoredTree> {
        StoredTree::open(self, self.resolve_band_id(band_selection).await?).await
    }

    /// Return the `BandId` of the highest-numbered band, or Ok(None) if there
    /// are no bands, or an Err if any occurred reading the directory.
    pub async fn last_band_id(&self) -> Result<Option<BandId>> {
        Ok(self.list_band_ids().await?.into_iter().max())
    }

    /// Return the last completely-written band id, if any.
    pub async fn last_complete_band(&self) -> Result<Option<Band>> {
        for band_id in self.list_band_ids().await?.into_iter().rev() {
            let b = Band::open(self, band_id).await?;
            if b.is_closed().await? {
                return Ok(Some(b));
            }
        }
        Ok(None)
    }

    /// Returns all blocks referenced by all bands.
    ///
    /// Shows a progress bar as they're collected.
    pub async fn referenced_blocks(
        &self,
        band_ids: &[BandId],
        monitor: Arc<dyn Monitor>,
    ) -> Result<HashSet<BlockHash>> {
        let archive = self.clone();
        let task = monitor.start_task("Find referenced blocks".to_string());
        let mut blocks = HashSet::new();
        for band_id in band_ids {
            let band = Band::open(&archive, *band_id).await?;
            let mut iter = band.index().iter_available_hunks().await;
            while let Some(hunk) = iter.next().await {
                for addr in hunk.into_iter().flat_map(|entry| entry.addrs) {
                    blocks.insert(addr.hash);
                    task.increment(1);
                }
            }
        }
        Ok(blocks)
    }

    /// Returns an iterator of blocks that are present and referenced by no index.
    pub async fn unreferenced_blocks(&self, monitor: Arc<dyn Monitor>) -> Result<Vec<BlockHash>> {
        let referenced = self
            .referenced_blocks(&self.list_band_ids().await?, monitor.clone())
            .await?;
        Ok(self
            .block_dir
            .blocks(monitor)
            .await?
            .into_iter()
            .filter(move |h| !referenced.contains(h))
            .collect())
    }

    /// Delete bands, and the blocks that they reference.
    ///
    /// If `delete_band_ids` is empty, this deletes no bands, but will delete any garbage
    /// blocks referenced by no existing bands.
    pub async fn delete_bands(
        &self,
        delete_band_ids: &[BandId],
        options: &DeleteOptions,
        monitor: Arc<dyn Monitor>,
    ) -> Result<DeleteStats> {
        let mut stats = DeleteStats::default();
        let start = Instant::now();

        // TODO: No need to lock for dry_run.
        let gc_lock = if options.break_lock {
            gc_lock::GarbageCollectionLock::break_lock(self).await?
        } else {
            gc_lock::GarbageCollectionLock::new(self).await?
        };
        debug!("Got gc lock");

        debug!("List band ids...");
        let mut keep_band_ids = self.list_band_ids().await?;
        keep_band_ids.retain(|b| !delete_band_ids.contains(b));

        debug!("List referenced blocks...");
        let referenced = self
            .referenced_blocks(&keep_band_ids, monitor.clone())
            .await?;
        debug!(referenced.len = referenced.len());

        debug!("Find present blocks...");
        let present: HashSet<BlockHash> = self
            .block_dir
            .blocks(monitor.clone())
            .await?
            .into_iter()
            .collect();
        debug!(present.len = present.len());

        debug!("Find unreferenced blocks...");
        let unref = present.difference(&referenced).collect_vec();
        let unref_count = unref.len();
        debug!(unref_count);
        stats.unreferenced_block_count = unref_count;

        debug!("Measure unreferenced blocks...");
        let task = monitor.start_task("Measure unreferenced blocks".to_string());
        task.set_total(unref_count);
        // TODO: Parallelize
        let mut total_bytes = 0;
        for block_id in &unref {
            total_bytes += self.block_dir.compressed_size(block_id).await?;
            task.increment(1);
        }
        drop(task);
        stats.unreferenced_block_bytes = total_bytes;

        if !options.dry_run {
            gc_lock.check().await?;
            let task = monitor.start_task("Delete bands".to_string());

            for band_id in delete_band_ids.iter() {
                Band::delete(self, *band_id).await?;
                stats.deleted_band_count += 1;
                task.increment(1);
            }

            let task = monitor.start_task("Delete blocks".to_string());
            task.set_total(unref_count);
            let mut error_count = 0;
            for block_hash in unref {
                task.increment(1);
                error_count += self.block_dir.delete_block(block_hash).await.is_err() as usize;
            }
            stats.deletion_errors += error_count;
            stats.deleted_block_count += unref_count - error_count;
        }
        gc_lock.release().await?;

        stats.elapsed = start.elapsed();
        Ok(stats)
    }

    /// Walk the archive to check all invariants.
    ///
    /// If problems are found, they are emitted as `warn` or `error` level
    /// tracing messages. This function only returns an error if validation
    /// stops due to a fatal error.
    pub async fn validate(
        &self,
        options: &ValidateOptions,
        monitor: Arc<dyn Monitor>,
    ) -> Result<()> {
        self.validate_archive_dir(monitor.clone()).await?;

        debug!("List bands...");
        let band_ids = self.list_band_ids().await?;
        debug!("Check {} bands...", band_ids.len());

        // 1. Walk all indexes, collecting a list of (block_hash6, min_length)
        //    values referenced by all the indexes.
        let referenced_lens = validate::validate_bands(self, &band_ids, monitor.clone()).await?;

        if options.skip_block_hashes {
            // 3a. Check that all referenced blocks are present, without spending time reading their
            // content.
            debug!("List blocks...");
            // TODO: Check for unexpected files or directories in the blockdir.
            let present_blocks: HashSet<BlockHash> = self
                .block_dir
                .blocks_async(monitor.clone())
                .await?
                .into_iter()
                .collect();
            for hash in referenced_lens.keys() {
                if !present_blocks.contains(hash) {
                    monitor.error(Error::BlockMissing { hash: hash.clone() })
                }
            }
        } else {
            // 2. Check the hash of all blocks are correct, and remember how long
            //    the uncompressed data is.
            let block_lengths: HashMap<BlockHash, usize> =
                self.block_dir.validate(monitor.clone()).await?;
            // 3b. Check that all referenced ranges are inside the present data.
            for (hash, referenced_len) in referenced_lens {
                if let Some(&actual_len) = block_lengths.get(&hash) {
                    if referenced_len > actual_len as u64 {
                        monitor.error(Error::BlockTooShort {
                            hash: hash.clone(),
                            actual_len,
                            referenced_len: referenced_len as usize,
                        });
                    }
                } else {
                    monitor.error(Error::BlockMissing { hash: hash.clone() })
                }
            }
        }
        Ok(())
    }

    async fn validate_archive_dir(&self, monitor: Arc<dyn Monitor>) -> Result<()> {
        // TODO: More tests for the problems detected here.
        debug!("Check archive directory...");
        let mut seen_bands = HashSet::<BandId>::new();
        let list_dir = self.transport.list_dir("").await?;
        for dir_name in list_dir.dirs {
            if let Ok(band_id) = dir_name.parse::<BandId>() {
                if !seen_bands.insert(band_id) {
                    // TODO: Test this
                    monitor.error(Error::InvalidMetadata {
                        details: format!("Duplicated band directory for {band_id:?}"),
                    });
                }
            } else if !dir_name.eq_ignore_ascii_case(BLOCK_DIR) {
                // TODO: The whole path not just the filename
                warn!(
                    path = dir_name,
                    "Unexpected subdirectory in archive directory"
                );
            }
        }
        for name in list_dir.files {
            if !name.eq_ignore_ascii_case(HEADER_FILENAME)
                && !name.eq_ignore_ascii_case(crate::gc_lock::GC_LOCK)
                && !name.eq_ignore_ascii_case(".DS_Store")
            {
                // TODO: The whole path not just the filename
                warn!(path = name, "Unexpected file in archive directory");
            }
        }
        Ok(())
    }
}
