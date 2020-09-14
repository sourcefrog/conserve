// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

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

use std::collections::{BTreeSet, HashMap};
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Mutex;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::backup::BackupOptions;
use crate::blockhash::BlockHash;
use crate::copy_tree::CopyOptions;
use crate::errors::Error;
use crate::jsonio::{read_json, write_json};
use crate::kind::Kind;
use crate::misc::remove_item;
use crate::stats::{CopyStats, ValidateStats};
use crate::stitch::IterStitchedIndexHunks;
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

    transport: Box<dyn Transport>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ArchiveHeader {
    conserve_archive_version: String,
}

impl Archive {
    /// Make a new archive in a local direcotry.
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
            transport,
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
            transport,
        })
    }

    /// Backup a source directory into a new band in the archive.
    ///
    /// Returns statistics about what was copied.
    pub fn backup(&self, source_path: &Path, options: &BackupOptions) -> Result<CopyStats> {
        let live_tree = LiveTree::open(source_path)?.with_excludes(options.excludes.clone());
        let writer = BackupWriter::begin(self)?;
        copy_tree(
            &live_tree,
            writer,
            &CopyOptions {
                print_filenames: options.print_filenames,
                measure_first: false,
                ..CopyOptions::default()
            },
        )
    }

    /// Restore a selected version, or by default the latest, to a destination directory.
    pub fn restore(&self, destination_path: &Path, options: &RestoreOptions) -> Result<CopyStats> {
        let st = self.open_stored_tree(options.band_selection.clone())?;
        let st = st.with_excludes(options.excludes.clone());
        let rt = if options.overwrite {
            RestoreTree::create_overwrite(destination_path)
        } else {
            RestoreTree::create(destination_path)
        }?;
        let opts = CopyOptions {
            print_filenames: options.print_filenames,
            only_subtree: options.only_subtree.clone(),
            ..CopyOptions::default()
        };
        copy_tree(&st, rt, &opts)
    }

    pub fn block_dir(&self) -> &BlockDir {
        &self.block_dir
    }

    pub fn band_exists(&self, band_id: &BandId) -> Result<bool> {
        self.transport
            .exists(&format!(
                "{}/{}",
                band_id.to_string(),
                crate::BAND_HEAD_FILENAME
            ))
            .map_err(Error::from)
    }

    pub fn band_is_closed(&self, band_id: &BandId) -> Result<bool> {
        self.transport
            .exists(&format!(
                "{}/{}",
                band_id.to_string(),
                crate::BAND_TAIL_FILENAME
            ))
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
            let b = Band::open(self, &id)?;
            if b.is_closed()? {
                return Ok(Some(b));
            }
        }
        Ok(None)
    }

    /// Returns all blocks referenced by all bands.
    ///
    /// Shows a progress bar as they're collected.
    pub fn referenced_blocks(&self) -> Result<BTreeSet<BlockHash>> {
        self.iter_referenced_blocks().map(Iterator::collect)
    }

    /// Iterate all blocks referenced by all bands.
    ///
    /// The iterator returns repeatedly-referenced blocks repeatedly, without deduplicating.
    ///
    /// This shows a progress bar as indexes are iterated.
    fn iter_referenced_blocks(&self) -> Result<impl Iterator<Item = BlockHash>> {
        let archive = self.clone();
        let mut progress_bar = ProgressBar::new();
        progress_bar.set_phase("Find referenced blocks...".to_owned());
        let band_ids = self.list_band_ids()?;
        let num_bands = band_ids.len();
        Ok(band_ids
            .into_iter()
            .enumerate()
            .inspect(move |(i, _)| progress_bar.set_fraction(*i, num_bands))
            .map(move |(_i, band_id)| Band::open(&archive, &band_id).expect("Failed to open band"))
            .flat_map(|band| band.iter_entries().expect("Failed to iter entries"))
            .flat_map(|entry| entry.addrs)
            .map(|addr| addr.hash))
    }

    /// Returns an iterator of blocks that are present and referenced by no index.
    pub fn unreferenced_blocks(&self) -> Result<impl Iterator<Item = BlockHash>> {
        let referenced = self.referenced_blocks()?;
        Ok(self
            .iter_present_blocks()?
            .filter(move |hash| !referenced.contains(hash)))
    }

    fn iter_present_blocks(&self) -> Result<impl Iterator<Item = BlockHash>> {
        let mut progress_bar = ProgressBar::new();
        progress_bar.set_phase("Find present blocks...".to_owned());
        Ok(self
            .block_dir()
            .block_names()?
            .inspect(move |_| progress_bar.increment_work_done(1)))
    }

    /// Delete unreferenced blocks.
    pub fn delete_unreferenced(&self, dry_run: bool) -> Result<DeleteUnreferencedStats> {
        let block_dir = self.block_dir();
        let mut stats = DeleteUnreferencedStats::default();
        let delete_guard = gc_lock::GarbageCollectionLock::new(self)?;

        let mut blocks: BTreeSet<BlockHash> = self.iter_present_blocks()?.collect();
        for block_hash in self.iter_referenced_blocks()? {
            // NOTE: We could potentially notice here blocks that are missing: referenced but
            // not present. However, because the reference iter can contain duplicates,
            // it would require keeping another set. On the whole that seems better left
            // to validation.
            blocks.remove(&block_hash);
        }
        stats.unreferenced_block_count = blocks.len();

        let mut progress_bar = ProgressBar::new();
        progress_bar.set_phase("Measure unreferenced blocks".to_owned());
        progress_bar.set_total_work(blocks.len());
        let total_bytes = blocks
            .iter()
            .inspect(|_| progress_bar.increment_work_done(1))
            .map(|hash| block_dir.compressed_size(hash).unwrap_or_default())
            .sum();
        stats.unreferenced_block_bytes = total_bytes;

        delete_guard.check()?;

        if !blocks.is_empty() {
            let mut progress_bar = ProgressBar::new();
            progress_bar.set_phase("Deleting unreferenced blocks".to_owned());
            progress_bar.set_total_work(blocks.len());
            for block_hash in blocks {
                if !dry_run {
                    if block_dir.delete_block(&block_hash).is_err() {
                        stats.deletion_errors += 1;
                    } else {
                        stats.deleted_block_count += 1;
                    }
                }
                progress_bar.increment_work_done(1);
            }
        }

        Ok(stats)
    }

    pub fn validate(&self) -> Result<ValidateStats> {
        let mut stats = self.validate_archive_dir()?;
        ui::println("Check blockdir...");
        let block_lengths: HashMap<BlockHash, usize> = self.block_dir.validate(&mut stats)?;

        ui::println("Check indexes...");
        let band_ids = self.list_band_ids()?;
        let num_bands = band_ids.len();

        let mut progress_bar = ProgressBar::new();
        progress_bar.set_phase("Check index".to_owned());
        progress_bar.set_total_work(num_bands);
        let progress_bar_mutex = Mutex::new(progress_bar);

        stats += band_ids
            .into_par_iter()
            .map(|band_id| {
                let mut stats = ValidateStats::default();

                if let Ok(b) = Band::open(self, &band_id) {
                    if b.validate(&mut stats).is_err() {
                        stats.band_metadata_problems += 1;
                    }
                } else {
                    stats.band_open_errors += 1;
                }

                if let Ok(st) = self.open_stored_tree(BandSelectionPolicy::Specified(band_id)) {
                    if st.validate(&block_lengths, &mut stats).is_err() {
                        stats.tree_validate_errors += 1
                    }
                } else {
                    stats.tree_open_errors += 1
                }

                if let Ok(mut progress_bar_lock) = progress_bar_mutex.lock() {
                    progress_bar_lock.increment_work_done(1);
                }
                stats
            })
            .reduce(|| ValidateStats::default(), |a, b| a + b);

        Ok(stats)
    }

    fn validate_archive_dir(&self) -> Result<ValidateStats> {
        // TODO: Tests for the problems detected here.
        let mut stats = ValidateStats::default();
        ui::println("Check archive top-level directory...");

        let mut files: Vec<String> = Vec::new();
        let mut dirs: Vec<String> = Vec::new();
        for entry_result in self
            .transport
            .iter_dir_entries("")
            .map_err(|source| Error::ListBands { source })?
        {
            match entry_result {
                Ok(DirEntry { name, kind, .. }) => match kind {
                    Kind::Dir => dirs.push(name),
                    Kind::File => files.push(name),
                    other_kind => {
                        ui::problem(&format!(
                            "Unexpected file kind in archive directory: {:?} of kind {:?}",
                            name, other_kind
                        ));
                        stats.unexpected_files += 1;
                    }
                },
                Err(source) => {
                    ui::problem(&format!("Error listing archive directory: {:?}", source));
                    stats.io_errors += 1;
                }
            }
        }
        remove_item(&mut files, &HEADER_FILENAME);
        if !files.is_empty() {
            stats.unexpected_files += 1;
            ui::problem(&format!(
                "Unexpected files in archive directory {:?}: {:?}",
                self.transport, files
            ));
        }
        remove_item(&mut dirs, &BLOCK_DIR);
        dirs.sort();
        let mut bs = BTreeSet::<BandId>::new();
        for d in dirs.iter() {
            if let Ok(b) = d.parse() {
                if bs.contains(&b) {
                    stats.structure_problems += 1;
                    ui::problem(&format!(
                        "Duplicated band directory in {:?}: {:?}",
                        self.transport, d
                    ));
                } else {
                    bs.insert(b);
                }
            } else {
                stats.structure_problems += 1;
                ui::problem(&format!(
                    "Unexpected directory in {:?}: {:?}",
                    self.transport, d
                ));
            }
        }
        Ok(stats)
    }

    /// Return an iterator that reconstructs the most complete available index for a possibly-incomplete band.
    ///
    /// If the band is complete, this is simply the band's index.
    ///
    /// If it's incomplete, it stitches together the index by picking up at the same point in the previous
    /// band, continuing backwards recursively until either there are no more previous indexes, or a complete
    /// index is found.
    pub fn iter_stitched_index_hunks(&self, band_id: &BandId) -> IterStitchedIndexHunks {
        IterStitchedIndexHunks::new(self, band_id)
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

        let result = Archive::create_path(&temp.path());
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
        let mut header_file = fs::File::open(&header_path).unwrap();
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
        assert_eq!(af.referenced_blocks().unwrap().iter().count(), 0);
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

        assert_eq!(af.referenced_blocks().unwrap().len(), 0);
        assert_eq!(af.block_dir.block_names().unwrap().count(), 0);
    }
}
