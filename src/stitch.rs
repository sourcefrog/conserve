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
    /// This moves back to earlier bands when we reach the end of an incomplete band.
    /// Might be none, if no more bands are available.
    band_id: Option<BandId>,

    /// The latest (and highest-ordered) apath we have already yielded.
    last_apath: Option<Apath>,

    /// Currently pending index hunks.
    index_hunks: Option<crate::index::IndexHunkIter>,

    archive: Archive,
}

impl IterStitchedIndexHunks {
    /// Return an iterator that reconstructs the most complete available index
    /// for a possibly-incomplete band.
    ///
    /// If the band is complete, this is simply the band's index.
    ///
    /// If it's incomplete, it stitches together the index by picking up at
    /// the same point in the previous band, continuing backwards recursively
    /// until either there are no more previous indexes, or a complete index
    /// is found.
    pub(crate) fn new(archive: &Archive, band_id: Option<BandId>) -> IterStitchedIndexHunks {
        IterStitchedIndexHunks {
            archive: archive.clone(),
            band_id,
            last_apath: None,
            index_hunks: None,
        }
    }

    pub fn iter_entries(
        self,
        subtree: Apath,
        exclude: Exclude,
    ) -> IndexEntryIter<IterStitchedIndexHunks> {
        IndexEntryIter::new(self, subtree, exclude)
    }
}

impl Iterator for IterStitchedIndexHunks {
    type Item = Vec<IndexEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we're already reading an index, and it has more content, return that.
            if let Some(index_hunks) = &mut self.index_hunks {
                // An index iterator must be assigned to a band.
                debug_assert!(self.band_id.is_some());

                for hunk in index_hunks {
                    if let Some(last_entry) = hunk.last() {
                        self.last_apath = Some(last_entry.apath().clone());
                        return Some(hunk);
                    } // otherwise, empty, try the next
                }
                // There are no more index hunks in the current band.
                self.index_hunks = None;

                let band_id = self.band_id.take().expect("last band id should be present");
                if self.archive.band_is_closed(&band_id).unwrap_or(false) {
                    // We reached the end of a complete index in this band,
                    // so there's no need to look at any earlier bands, and we're done iterating.
                    return None;
                }

                // self.band_id might be None afterwards, if there is no previous band.
                // If so, we're done.
                self.band_id = previous_existing_band(&self.archive, &band_id);
            }

            if let Some(band_id) = &self.band_id {
                // Start reading this new index and skip forward until after last_apath
                let mut iter_hunks = Band::open(&self.archive, band_id)
                    .expect("Failed to open band")
                    .index()
                    .iter_hunks();

                if let Some(last) = &self.last_apath {
                    iter_hunks = iter_hunks.advance_to_after(last)
                }
                self.index_hunks = Some(iter_hunks);
            } else {
                // We got no more bands with possible new index information.
                return None;
            }
        }
    }
}

fn previous_existing_band(archive: &Archive, band_id: &BandId) -> Option<BandId> {
    let mut band_id = band_id.clone();
    loop {
        // TODO: It might be faster to list the present bands and calculate
        // from that, rather than walking backwards one at a time...
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
    use super::*;
    use crate::test_fixtures::{ScratchArchive, TreeFixture};

    fn symlink(name: &str, target: &str) -> IndexEntry {
        IndexEntry {
            apath: name.into(),
            kind: Kind::Symlink,
            target: Some(target.to_owned()),
            mtime: 0,
            mtime_nanos: 0,
            addrs: Vec::new(),
            unix_mode: Default::default(),
            owner: Default::default(),
        }
    }

    fn simple_ls(archive: &Archive, band_id: &BandId) -> String {
        let strs: Vec<String> = IterStitchedIndexHunks::new(archive, Some(band_id.clone()))
            .flatten()
            .map(|entry| format!("{}:{}", &entry.apath, entry.target.unwrap()))
            .collect();
        strs.join(" ")
    }

    #[test]
    fn stitch_index() -> Result<()> {
        // This test uses private interfaces to create an index that breaks
        // across hunks in a certain way.

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

        std::fs::remove_dir_all(af.path().join("b0003"))?;

        let archive = Archive::open_path(af.path())?;
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

    /// Testing that the StitchedIndexHunks iterator does not loops forever on archives with at least one band
    /// but no completed bands.
    /// Reference: https://github.com/sourcefrog/conserve/pull/175
    #[test]
    fn issue_175() {
        let tf = TreeFixture::new();
        tf.create_file("file_a");

        let lt = tf.live_tree();
        let af = ScratchArchive::new();
        backup(&af, &lt, &BackupOptions::default(), None).expect("backup should work");

        af.transport().remove_file("b0000/BANDTAIL").unwrap();
        let band_ids = af.list_band_ids().expect("should list bands");

        let band_id = band_ids.first().expect("expected at least one band");

        let mut iter = IterStitchedIndexHunks::new(&af, Some(band_id.clone()));
        // Get the first and only index entry.
        // `index_hunks` and `band_id` should be `Some`.
        assert!(iter.next().is_some());

        // Remove the band head. This band can not be opened anymore.
        // If accessed this should fail the test.
        // Note: When refactoring `.expect("Failed to open band")` this might needs refactoring as well.
        af.transport().remove_file("b0000/BANDHEAD").unwrap();

        // No more entries should follow.
        for _ in 0..10 {
            assert!(iter.next().is_none());
        }
    }
}
