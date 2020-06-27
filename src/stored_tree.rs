// Copyright 2017, 2018, 2019, 2020 Martin Pool.

//! Access a versioned tree stored in the archive.
//!
//! Through this interface you can iterate the contents and retrieve file contents.
//!
//! This is the preferred higher-level interface for reading stored versions. It'll abstract
//! across incremental backups, hiding from the caller that data may be distributed across
//! multiple index files, bands, and blocks.

use std::collections::HashMap;

use rayon::iter::ParallelBridge;
use rayon::prelude::*;

use crate::blockdir::BlockDir;
use crate::kind::Kind;
use crate::stored_file::{ReadStoredFile, StoredFile};
use crate::*;

/// Read index and file contents for a version stored in the archive.
pub struct StoredTree {
    band: Band,
    block_dir: BlockDir,
    excludes: GlobSet,
}

impl StoredTree {
    /// Open the last complete version in the archive.
    pub fn open_last(archive: &Archive) -> Result<StoredTree> {
        let band = archive
            .last_complete_band()?
            .ok_or(errors::Error::ArchiveEmpty)?;
        Ok(StoredTree {
            band,
            block_dir: archive.block_dir().clone(),
            excludes: excludes::excludes_nothing(),
        })
    }

    /// Open a specified version.
    ///
    /// It's an error if it's not complete.
    pub fn open_version(archive: &Archive, band_id: &BandId) -> Result<StoredTree> {
        let band = Band::open(archive, band_id)?;
        if !band.is_closed()? {
            return Err(Error::BandIncomplete {
                band_id: band_id.clone(),
            });
        }
        Ok(StoredTree {
            band,
            block_dir: archive.block_dir().clone(),
            excludes: excludes::excludes_nothing(),
        })
    }

    /// Open a specified version.
    ///
    /// This function allows opening incomplete versions, which might contain only a partial copy
    /// of the source tree, or maybe nothing at all.
    pub fn open_incomplete_version(archive: &Archive, band_id: &BandId) -> Result<StoredTree> {
        let band = Band::open(archive, band_id)?;
        Ok(StoredTree {
            band,
            block_dir: archive.block_dir().clone(),
            excludes: excludes::excludes_nothing(),
        })
    }

    pub fn with_excludes(self, excludes: GlobSet) -> StoredTree {
        StoredTree { excludes, ..self }
    }

    pub fn band(&self) -> &Band {
        &self.band
    }

    pub fn is_closed(&self) -> Result<bool> {
        self.band.is_closed()
    }

    pub fn validate(
        &self,
        block_lens: &HashMap<String, usize>,
        stats: &mut ValidateStats,
    ) -> Result<()> {
        ui::set_progress_phase(&format!("Check tree {}", self.band().id()));
        stats.block_missing_count = self
            .iter_entries()?
            .par_bridge()
            .filter(|entry| entry.kind() == Kind::File)
            .flat_map(|entry| entry.addrs)
            .filter(|addr| {
                if let Some(block_len) = block_lens.get(&addr.hash) {
                    // Present, but the address is out of range.
                    if (addr.start + addr.len) > (*block_len as u64) {
                        ui::problem(&format!(
                            "Address {:?} extends beyond compressed data length {}",
                            addr, block_len
                        ));
                        true
                    } else {
                        false
                    }
                } else {
                    ui::problem(&format!("Address {:?} points to missing block", addr));
                    true
                }
            })
            .count();
        Ok(())
    }

    /// Open a file stored within this tree.
    fn open_stored_file(&self, entry: &IndexEntry) -> Result<StoredFile> {
        Ok(StoredFile::open(
            self.block_dir.clone(),
            entry.addrs.clone(),
        ))
    }
}

impl ReadTree for StoredTree {
    type I = index::IndexEntryIter;
    type R = ReadStoredFile;
    type Entry = IndexEntry;

    /// Return an iter of index entries in this stored tree.
    fn iter_entries(&self) -> Result<index::IndexEntryIter> {
        Ok(self
            .band
            .iter_entries()?
            .with_excludes(self.excludes.clone()))
    }

    fn file_contents(&self, entry: &Self::Entry) -> Result<Self::R> {
        Ok(self.open_stored_file(entry)?.into_read())
    }

    fn estimate_count(&self) -> Result<u64> {
        self.band.index().estimate_entry_count()
    }
}

#[cfg(test)]
mod test {
    use super::super::test_fixtures::*;
    use super::super::*;

    #[test]
    pub fn open_stored_tree() {
        let af = ScratchArchive::new();
        af.store_two_versions();

        let last_band_id = af.last_band_id().unwrap().unwrap();
        let st = StoredTree::open_last(&af).unwrap();

        assert_eq!(*st.band().id(), last_band_id);

        let names: Vec<String> = st.iter_entries().unwrap().map(|e| e.apath.into()).collect();
        let expected = if SYMLINKS_SUPPORTED {
            vec![
                "/",
                "/hello",
                "/hello2",
                "/link",
                "/subdir",
                "/subdir/subfile",
            ]
        } else {
            vec!["/", "/hello", "/hello2", "/subdir", "/subdir/subfile"]
        };
        assert_eq!(expected, names);
    }

    #[test]
    pub fn cant_open_no_versions() {
        let af = ScratchArchive::new();
        assert!(StoredTree::open_last(&af).is_err());
    }
}
