// Copyright 2017, 2018, 2019, 2020, 2021 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Access a versioned tree stored in the archive.
//!
//! Through this interface you can iterate the contents and retrieve file contents.
//!
//! This is the preferred higher-level interface for reading stored versions. It'll abstract
//! across incremental backups, hiding from the caller that data may be distributed across
//! multiple index files, bands, and blocks.

use crate::blockdir::BlockDir;
use crate::stitch::IterStitchedIndexHunks;
use crate::stored_file::{ReadStoredFile, StoredFile};
use crate::*;

/// Read index and file contents for a version stored in the archive.
pub struct StoredTree {
    band: Band,
    archive: Archive,
    block_dir: BlockDir,
}

impl StoredTree {
    pub(crate) fn open(archive: &Archive, band_id: &BandId) -> Result<StoredTree> {
        Ok(StoredTree {
            band: Band::open(archive, band_id)?,
            block_dir: archive.block_dir().clone(),
            archive: archive.clone(),
        })
    }

    pub fn band(&self) -> &Band {
        &self.band
    }

    pub fn is_closed(&self) -> Result<bool> {
        self.band.is_closed()
    }

    /// Open a file stored within this tree.
    fn open_stored_file(&self, entry: &IndexEntry) -> StoredFile {
        StoredFile::open(self.block_dir.clone(), entry.addrs.clone())
    }
}

impl ReadTree for StoredTree {
    type R = ReadStoredFile;
    type Entry = IndexEntry;
    type IT = index::IndexEntryIter<stitch::IterStitchedIndexHunks>;

    /// Return an iter of index entries in this stored tree.
    fn iter_entries(&self, subtree: Apath, exclude: Exclude) -> Result<Self::IT> {
        Ok(
            IterStitchedIndexHunks::new(&self.archive, Some(self.band.id().clone()))
                .iter_entries(subtree, exclude),
        )
    }

    fn file_contents(&self, entry: &Self::Entry) -> Result<Self::R> {
        Ok(self.open_stored_file(entry).into_read())
    }

    fn estimate_count(&self) -> Result<u64> {
        self.band.index().estimate_entry_count()
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use super::super::test_fixtures::*;
    use super::super::*;

    #[test]
    pub fn open_stored_tree() {
        let af = ScratchArchive::new();
        af.store_two_versions();

        let last_band_id = af.last_band_id().unwrap().unwrap();
        let st = af.open_stored_tree(BandSelectionPolicy::Latest).unwrap();

        assert_eq!(*st.band().id(), last_band_id);

        let names: Vec<String> = st
            .iter_entries(Apath::root(), Exclude::nothing())
            .unwrap()
            .map(|e| e.apath.into())
            .collect();
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
        match af.open_stored_tree(BandSelectionPolicy::Latest) {
            Err(Error::ArchiveEmpty) => (),
            Err(other) => panic!("unexpected result {other:?}"),
            Ok(_) => panic!("unexpected success"),
        }
    }

    #[test]
    fn iter_entries() {
        let archive = Archive::open_path(Path::new("testdata/archive/minimal/v0.6.3/")).unwrap();
        let st = archive
            .open_stored_tree(BandSelectionPolicy::Latest)
            .unwrap();

        let names: Vec<String> = st
            .iter_entries("/subdir".into(), Exclude::nothing())
            .unwrap()
            .map(|entry| entry.apath.into())
            .collect();

        assert_eq!(names.as_slice(), ["/subdir", "/subdir/subfile"]);
    }
}
