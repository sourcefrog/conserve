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

//! Stitch together any number of incomplete indexes to form a more-complete
//! index.
//!
//! If a backup is interrupted, we may have several index hunks (and their
//! referenced blocks) but not a complete tree. The best tree to restore at
//! that point is the new index blocks, for as much of the tree as they cover,
//! and then the next older index from that apath onwards.  This can be applied
//! recursively if the next-older index was also incomplete, until we either
//! reach a complete index (i.e. one with a tail), or there are no more older
//! indexes.
//!
//! In doing this we need to be careful of a couple of edge cases:
//!
//! * The next-older index might end at an earlier apath than we've already
//!   seen.
//! * Bands might be deleted, so their numbers are not contiguous.

use crate::index::IndexEntryIter;
use crate::*;

pub struct IterStitchedIndexHunks {
    /// Current band_id: initially the requested band_id.
    band_id: BandId,

    /// The latest (and highest-ordered) apath we have already yielded.
    last_apath: Option<Apath>,

    /// Currently pending index hunks.
    index_hunks: Option<crate::index::IndexHunkIter>,

    archive: Archive,
}

impl IterStitchedIndexHunks {
    pub(crate) fn new(archive: &Archive, band_id: &BandId) -> IterStitchedIndexHunks {
        IterStitchedIndexHunks {
            archive: archive.clone(),
            band_id: band_id.clone(),
            last_apath: None,
            index_hunks: None,
        }
    }

    pub fn iter_entries(
        self,
        subtree: Option<Apath>,
        excludes: Option<GlobSet>,
    ) -> IndexEntryIter<IterStitchedIndexHunks> {
        IndexEntryIter::new(self, subtree, excludes)
    }
}

impl Iterator for IterStitchedIndexHunks {
    type Item = Vec<IndexEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we're already reading an index, and it has more content, return that.
            if let Some(index_hunks) = &mut self.index_hunks {
                for hunk in index_hunks {
                    if let Some(last_entry) = hunk.last() {
                        self.last_apath = Some(last_entry.apath().clone());
                        return Some(hunk);
                    } // otherwise, empty, try the next
                }
                if self.archive.band_is_closed(&self.band_id).unwrap_or(false) {
                    return None;
                }
                self.index_hunks = None;
                if let Some(band_id) = previous_existing_band(&self.archive, &self.band_id) {
                    self.band_id = band_id;
                } else {
                    return None;
                }
            }
            // Start reading this new index and skip forward until after last_apath
            let mut iter_hunks = Band::open(&self.archive, &self.band_id)
                .expect("Failed to open band")
                .index()
                .iter_hunks();
            if let Some(last) = &self.last_apath {
                iter_hunks = iter_hunks.advance_to_after(last)
            }
            self.index_hunks = Some(iter_hunks);
        }
    }
}

fn previous_existing_band(archive: &Archive, band_id: &BandId) -> Option<BandId> {
    let mut band_id = band_id.clone();
    loop {
        if let Some(prev_band_id) = band_id.previous() {
            band_id = prev_band_id;
            if archive.band_exists(&band_id).unwrap_or(false) {
                return Some(band_id);
            }
        } else {
            return None;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::test_fixtures::ScratchArchive;
    use crate::*;

    fn symlink(name: &str, target: &str) -> IndexEntry {
        IndexEntry {
            apath: name.into(),
            kind: Kind::Symlink,
            target: Some(target.to_owned()),
            mtime: 0,
            mtime_nanos: 0,
            addrs: Vec::new(),
        }
    }

    fn simple_ls(archive: &Archive, band_id: &BandId) -> String {
        let strs: Vec<String> = archive
            .iter_stitched_index_hunks(band_id)
            .flatten()
            .map(|entry| format!("{}:{}", &entry.apath, entry.target.unwrap()))
            .collect();
        strs.join(" ")
    }

    #[test]
    fn stitch_index() -> Result<()> {
        let af = ScratchArchive::new();

        // Construct a history with four versions:
        //
        // * b0 is incomplete and contains symlinks 0, 1, 2 all with target 'b0'.
        // * b1 is complete and contains symlinks 0, 1, 2, 3 all with target 'b1'.
        // * b2 is incomplete and contains symlinks 0, 2, with target 'b2'. 1 has been deleted, and 3
        //   we don't know about, so will assume is carried over.
        // * b3 has been deleted
        // * b4 exists but has no hunks.
        // * b5 is incomplete and contains symlink 0, 00, with target 'b5'.
        //   1 was deleted in b2, 2 is carried over from b2,
        //   and 3 is carried over from b1.

        let band = Band::create(&af)?;
        assert_eq!(*band.id(), BandId::zero());
        let mut ib = band.index_builder();
        ib.push_entry(symlink("/0", "b0"));
        ib.push_entry(symlink("/1", "b0"));
        ib.finish_hunk()?;
        ib.push_entry(symlink("/2", "b0"));
        // Flush this hunk but leave the band incomplete.
        let stats = ib.finish()?;
        assert_eq!(stats.index_hunks, 2);

        let band = Band::create(&af)?;
        assert_eq!(band.id().to_string(), "b0001");
        let mut ib = band.index_builder();
        ib.push_entry(symlink("/0", "b1"));
        ib.push_entry(symlink("/1", "b1"));
        ib.finish_hunk()?;
        ib.push_entry(symlink("/2", "b1"));
        ib.push_entry(symlink("/3", "b1"));
        let stats = ib.finish()?;
        assert_eq!(stats.index_hunks, 2);
        band.close(2)?;

        // b2
        let band = Band::create(&af)?;
        assert_eq!(band.id().to_string(), "b0002");
        let mut ib = band.index_builder();
        ib.push_entry(symlink("/0", "b2"));
        ib.finish_hunk()?;
        ib.push_entry(symlink("/2", "b2"));
        // incomplete
        let stats = ib.finish()?;
        assert_eq!(stats.index_hunks, 2);

        // b3
        let band = Band::create(&af)?;
        assert_eq!(band.id().to_string(), "b0003");

        // b4
        let band = Band::create(&af)?;
        assert_eq!(band.id().to_string(), "b0004");

        // b5
        let band = Band::create(&af)?;
        assert_eq!(band.id().to_string(), "b0005");
        let mut ib = band.index_builder();
        ib.push_entry(symlink("/0", "b5"));
        ib.push_entry(symlink("/00", "b5"));
        let stats = ib.finish()?;
        assert_eq!(stats.index_hunks, 1);
        // incomplete

        std::fs::remove_dir_all(&af.path().join("b0003"))?;

        let archive = Archive::open_path(&af.path())?;
        assert_eq!(simple_ls(&archive, &BandId::new(&[0])), "/0:b0 /1:b0 /2:b0");

        assert_eq!(
            simple_ls(&archive, &BandId::new(&[1])),
            "/0:b1 /1:b1 /2:b1 /3:b1"
        );

        assert_eq!(simple_ls(&archive, &BandId::new(&[2])), "/0:b2 /2:b2 /3:b1");

        assert_eq!(simple_ls(&archive, &BandId::new(&[4])), "/0:b2 /2:b2 /3:b1");

        assert_eq!(
            simple_ls(&archive, &BandId::new(&[5])),
            "/0:b5 /00:b5 /2:b2 /3:b1"
        );

        Ok(())
    }
}
