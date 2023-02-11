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

//! Archives holding backup material.

use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use itertools::Itertools;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};

use crate::blockhash::BlockHash;
use crate::errors::Error;
use crate::jsonio::{read_json, write_json};
use crate::kind::Kind;
use crate::progress::{Bar, Progress};
use crate::transport::local::LocalTransport;
use crate::transport::{DirEntry, Transport};
use crate::*;

const HEADER_FILENAME: &str = "CONSERVE";
static BLOCK_DIR: &str = "d";

/// An archive holding backup material.
#[derive(Clone, Debug)]
pub struct Archive {
    /// Holds body content for all file versions.
    block_dir: BlockDir,

    transport: Arc<dyn Transport>,
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
    pub fn create_path(path: &Path) -> Result<Archive> {
        Archive::create(Box::new(LocalTransport::new(path)))
    }

    /// Make a new archive in a new directory accessed by a Transport.
    pub fn create(transport: Box<dyn Transport>) -> Result<Archive> {
        transport
            .create_dir("")
            .map_err(|source| Error::CreateArchiveDirectory { source })?;
        let names = transport.list_dir_names("").map_err(Error::from)?;
        if !names.files.is_empty() || !names.dirs.is_empty() {
            return Err(Error::NewArchiveDirectoryNotEmpty);
        }
        let block_dir = BlockDir::create(transport.sub_transport(BLOCK_DIR))?;
        write_json(
            &transport,
            HEADER_FILENAME,
            &ArchiveHeader {
                conserve_archive_version: String::from(ARCHIVE_VERSION),
            },
        )?;
        Ok(Archive {
            block_dir,
            transport: Arc::from(transport),
        })
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open_path(path: &Path) -> Result<Archive> {
        Archive::open(Box::new(LocalTransport::new(path)))
    }

    pub fn open(transport: Box<dyn Transport>) -> Result<Archive> {
        let header: ArchiveHeader =
            read_json(&transport, HEADER_FILENAME).map_err(|err| match err {
                Error::MetadataNotFound { .. } => Error::NotAnArchive {},
                Error::IOError { source } if source.kind() == ErrorKind::NotFound => {
                    Error::NotAnArchive {}
                }
                Error::IOError { source } => Error::ReadArchiveHeader { source },
                other => other,
            })?;
        if header.conserve_archive_version != ARCHIVE_VERSION {
            return Err(Error::UnsupportedArchiveVersion {
                version: header.conserve_archive_version,
            });
        }
        let block_dir = BlockDir::open(transport.sub_transport(BLOCK_DIR));
        Ok(Archive {
            block_dir,
            transport: Arc::from(transport),
        })
    }

    pub fn block_dir(&self) -> &BlockDir {
        &self.block_dir
    }

    pub fn band_exists(&self, band_id: &BandId) -> Result<bool> {
        self.transport
            .is_file(&format!("{}/{}", band_id, crate::BAND_HEAD_FILENAME))
            .map_err(Error::from)
    }

    pub fn band_is_closed(&self, band_id: &BandId) -> Result<bool> {
        self.transport
            .is_file(&format!("{}/{}", band_id, crate::BAND_TAIL_FILENAME))
            .map_err(Error::from)
    }

    /// Returns a vector of band ids, in sorted order from first to last.
    pub fn list_band_ids(&self) -> Result<Vec<BandId>> {
        let mut band_ids: Vec<BandId> = self.iter_band_ids_unsorted()?.collect();
        band_ids.sort_unstable();
        Ok(band_ids)
    }

    pub(crate) fn transport(&self) -> &dyn Transport {
        self.transport.as_ref()
    }

    pub fn resolve_band_id(&self, band_selection: BandSelectionPolicy) -> Result<BandId> {
        match band_selection {
            BandSelectionPolicy::LatestClosed => self
                .last_complete_band()?
                .map(|band| band.id().clone())
                .ok_or(Error::ArchiveEmpty),
            BandSelectionPolicy::Specified(band_id) => Ok(band_id),
            BandSelectionPolicy::Latest => self.last_band_id()?.ok_or(Error::ArchiveEmpty),
        }
    }

    pub fn open_stored_tree(&self, band_selection: BandSelectionPolicy) -> Result<StoredTree> {
        StoredTree::open(self, &self.resolve_band_id(band_selection)?)
    }

    /// Return an iterator of valid band ids in this archive, in arbitrary order.
    ///
    /// Errors reading the archive directory are logged and discarded.
    fn iter_band_ids_unsorted(&self) -> Result<impl Iterator<Item = BandId>> {
        // This doesn't check for extraneous files or directories, which should be a weird rare
        // problem. Validate does.
        Ok(self
            .transport
            .list_dir_names("")
            .map_err(|source| Error::ListBands { source })?
            .dirs
            .into_iter()
            .filter(|dir_name| dir_name != BLOCK_DIR)
            .filter_map(|dir_name| dir_name.parse().ok()))
    }

    /// Return the `BandId` of the highest-numbered band, or Ok(None) if there
    /// are no bands, or an Err if any occurred reading the directory.
    pub fn last_band_id(&self) -> Result<Option<BandId>> {
        Ok(self.iter_band_ids_unsorted()?.max())
    }

    /// Return the last completely-written band id, if any.
    pub fn last_complete_band(&self) -> Result<Option<Band>> {
        for id in self.list_band_ids()?.iter().rev() {
            let b = Band::open(self, id)?;
            if b.is_closed()? {
                return Ok(Some(b));
            }
        }
        Ok(None)
    }

    /// Returns all blocks referenced by all bands.
    ///
    /// Shows a progress bar as they're collected.
    pub fn referenced_blocks(&self, band_ids: &[BandId]) -> Result<HashSet<BlockHash>> {
        let archive = self.clone();
        // TODO: Percentage completion based on how many bands have been checked so far.
        let bar = Bar::new();
        let references_found = AtomicUsize::new(0);
        let bands_started = AtomicUsize::new(0);
        let total_bands = band_ids.len();
        let start = Instant::now();
        Ok(band_ids
            .par_iter()
            .inspect(|_| {
                bands_started.fetch_add(1, Ordering::Relaxed);
            })
            .map(move |band_id| Band::open(&archive, band_id).expect("Failed to open band"))
            .flat_map_iter(|band| band.index().iter_entries())
            .flat_map_iter(|entry| entry.addrs)
            .map(|addr| addr.hash)
            .inspect(|_hash| {
                bar.post(Progress::ReferencedBlocks {
                    references_found: references_found.fetch_add(1, Ordering::Relaxed),
                    bands_started: bands_started.load(Ordering::Relaxed),
                    total_bands,
                    start,
                })
            })
            .collect())
    }

    /// Returns an iterator of blocks that are present and referenced by no index.
    pub fn unreferenced_blocks(&self) -> Result<impl Iterator<Item = BlockHash>> {
        let referenced = self.referenced_blocks(&self.list_band_ids()?)?;
        Ok(self
            .block_dir()
            .block_names()?
            .filter(move |h| !referenced.contains(h)))
    }

    /// Delete bands, and the blocks that they reference.
    ///
    /// If `delete_band_ids` is empty, this deletes no bands, but will delete any garbage
    /// blocks referenced by no existing bands.
    pub fn delete_bands(
        &self,
        delete_band_ids: &[BandId],
        options: &DeleteOptions,
    ) -> Result<DeleteStats> {
        let mut stats = DeleteStats::default();
        let start = Instant::now();

        // TODO: No need to lock for dry_run.
        let delete_guard = if options.break_lock {
            gc_lock::GarbageCollectionLock::break_lock(self)?
        } else {
            gc_lock::GarbageCollectionLock::new(self)?
        };
        debug!("Got gc lock");

        let block_dir = self.block_dir();
        debug!("List band ids...");
        let mut keep_band_ids = self.list_band_ids()?;
        keep_band_ids.retain(|b| !delete_band_ids.contains(b));

        debug!("List referenced blocks...");
        let referenced = self.referenced_blocks(&keep_band_ids)?;
        debug!(referenced.len = referenced.len());

        debug!("Find present blocks...");
        let present = self.block_dir.block_names_set()?;
        debug!(present.len = present.len());

        debug!("Find unreferenced blocks...");
        let unref = present.difference(&referenced).collect_vec();
        let unref_count = unref.len();
        debug!(unref_count);
        stats.unreferenced_block_count = unref_count;

        debug!("Measure unreferenced blocks...");
        let measure_bar = Bar::new();
        let total_bytes = unref
            .par_iter()
            .enumerate()
            .inspect(|(i, _)| {
                measure_bar.post(Progress::MeasureUnreferenced {
                    blocks_done: *i,
                    blocks_total: unref_count,
                })
            })
            .map(|(_i, block_id)| block_dir.compressed_size(block_id).unwrap_or_default())
            .sum();
        drop(measure_bar);
        stats.unreferenced_block_bytes = total_bytes;

        if !options.dry_run {
            delete_guard.check()?;
            let bar = Bar::new();

            for (bands_done, band_id) in delete_band_ids.iter().enumerate() {
                Band::delete(self, band_id)?;
                stats.deleted_band_count += 1;
                bar.post(Progress::DeleteBands {
                    bands_done,
                    total_bands: delete_band_ids.len(),
                });
            }

            let error_count = unref
                .par_iter()
                .enumerate()
                .inspect(|(blocks_done, _hash)| {
                    bar.post(Progress::DeleteBlocks {
                        blocks_done: *blocks_done,
                        total_blocks: unref_count,
                    })
                })
                .filter(|(_, block_hash)| block_dir.delete_block(block_hash).is_err())
                .count();
            stats.deletion_errors += error_count;
            stats.deleted_block_count += unref_count - error_count;
        }

        stats.elapsed = start.elapsed();
        Ok(stats)
    }

    /// Walk the archive to check all invariants.
    ///
    /// If problems are found, they are emitted as `warn` or `error` level
    /// tracing messages. This function only returns an error if validation
    /// stops due to a fatal error.
    pub fn validate(&self, options: &ValidateOptions) -> Result<()> {
        self.validate_archive_dir()?;

        debug!("List bands...");
        let band_ids = self.list_band_ids()?;
        debug!("Check {} bands...", band_ids.len());

        // 1. Walk all indexes, collecting a list of (block_hash6, min_length)
        //    values referenced by all the indexes.
        let referenced_lens = validate::validate_bands(self, &band_ids)?;

        if options.skip_block_hashes {
            // 3a. Check that all referenced blocks are present, without spending time reading their
            // content.
            debug!("List blocks...");
            // TODO: Check for unexpected files or directories in the blockdir.
            let present_blocks: HashSet<BlockHash> = self.block_dir.block_names_set()?;
            for block_hash in referenced_lens.keys() {
                if !present_blocks.contains(block_hash) {
                    error!(%block_hash, "Referenced block missing");
                }
            }
        } else {
            // 2. Check the hash of all blocks are correct, and remember how long
            //    the uncompressed data is.
            let block_lengths: HashMap<BlockHash, usize> = self.block_dir.validate()?;
            // 3b. Check that all referenced ranges are inside the present data.
            for (block_hash, referenced_len) in referenced_lens {
                if let Some(&actual_len) = block_lengths.get(&block_hash) {
                    if referenced_len > actual_len as u64 {
                        error!(
                            %block_hash,
                            referenced_len,
                            actual_len,
                            "Block is shorter than referenced length"
                        );
                    }
                } else {
                    error!(%block_hash, "Referenced block missing");
                }
            }
        }
        Ok(())
    }

    fn validate_archive_dir(&self) -> Result<()> {
        // TODO: More tests for the problems detected here.
        debug!("Check archive directory...");
        let mut seen_bands = HashSet::<BandId>::new();
        for entry_result in self
            .transport
            .iter_dir_entries("")
            .map_err(|source| Error::ListBands { source })?
        {
            match entry_result {
                Ok(DirEntry {
                    kind: Kind::Dir,
                    name,
                    ..
                }) => {
                    if let Ok(band_id) = name.parse::<BandId>() {
                        if !seen_bands.insert(band_id.clone()) {
                            // TODO: Test this
                            error!(%band_id, "Duplicated band directory");
                        }
                    } else if !name.eq_ignore_ascii_case(BLOCK_DIR) {
                        // TODO: The whole path not just the filename
                        warn!(path = name, "Unexpected subdirectory in archive directory");
                    }
                }
                Ok(DirEntry {
                    kind: Kind::File,
                    name,
                    ..
                }) => {
                    if !name.eq_ignore_ascii_case(HEADER_FILENAME)
                        && !name.eq_ignore_ascii_case(crate::gc_lock::GC_LOCK)
                        && !name.eq_ignore_ascii_case(".DS_Store")
                    {
                        // TODO: The whole path not just the filename
                        warn!(path = name, "Unexpected file in archive directory");
                    }
                }
                Ok(DirEntry { name, .. }) => {
                    // TODO: The whole path not just the filename
                    warn!(path = name, "Unexpected file in archive directory");
                }
                Err(err) => {
                    error!(%err, "Error listing archive directory");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;

    use assert_fs::prelude::*;
    use assert_fs::TempDir;

    use crate::test_fixtures::ScratchArchive;

    use super::*;

    #[test]
    fn create_then_open_archive() {
        let testdir = TempDir::new().unwrap();
        let arch_path = testdir.path().join("arch");
        let arch = Archive::create_path(&arch_path).unwrap();

        assert!(arch.list_band_ids().unwrap().is_empty());

        // We can re-open it.
        Archive::open_path(&arch_path).unwrap();
        assert!(arch.list_band_ids().unwrap().is_empty());
        assert!(arch.last_complete_band().unwrap().is_none());
    }

    #[test]
    fn fails_on_non_empty_directory() {
        let temp = TempDir::new().unwrap();

        temp.child("i am already here").touch().unwrap();

        let result = Archive::create_path(temp.path());
        assert!(result.is_err());
        if let Err(Error::NewArchiveDirectoryNotEmpty) = result {
        } else {
            panic!("expected an error for a non-empty new archive directory")
        }

        temp.close().unwrap();
    }

    /// A new archive contains just one header file.
    /// The header is readable json containing only a version number.
    #[test]
    fn empty_archive() {
        let af = ScratchArchive::new();

        assert!(af.path().is_dir());
        assert!(af.path().join("CONSERVE").is_file());
        assert!(af.path().join("d").is_dir());

        let header_path = af.path().join("CONSERVE");
        let mut header_file = fs::File::open(header_path).unwrap();
        let mut contents = String::new();
        header_file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "{\"conserve_archive_version\":\"0.6\"}\n");

        assert!(
            af.last_band_id().unwrap().is_none(),
            "Archive should have no bands yet"
        );
        assert!(
            af.last_complete_band().unwrap().is_none(),
            "Archive should have no bands yet"
        );
        assert_eq!(
            af.referenced_blocks(&af.list_band_ids().unwrap())
                .unwrap()
                .len(),
            0
        );
        assert_eq!(af.block_dir.block_names().unwrap().count(), 0);
    }

    #[test]
    fn create_bands() {
        let af = ScratchArchive::new();
        assert!(af.path().join("d").is_dir());

        // Make one band
        let _band1 = Band::create(&af).unwrap();
        let band_path = af.path().join("b0000");
        assert!(band_path.is_dir());
        assert!(band_path.join("BANDHEAD").is_file());
        assert!(band_path.join("i").is_dir());

        assert_eq!(af.list_band_ids().unwrap(), vec![BandId::new(&[0])]);
        assert_eq!(af.last_band_id().unwrap(), Some(BandId::new(&[0])));

        // Try creating a second band.
        let _band2 = Band::create(&af).unwrap();
        assert_eq!(
            af.list_band_ids().unwrap(),
            vec![BandId::new(&[0]), BandId::new(&[1])]
        );
        assert_eq!(af.last_band_id().unwrap(), Some(BandId::new(&[1])));

        assert_eq!(
            af.referenced_blocks(&af.list_band_ids().unwrap())
                .unwrap()
                .len(),
            0
        );
        assert_eq!(af.block_dir.block_names().unwrap().count(), 0);
    }
}
