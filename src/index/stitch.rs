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

use std::iter::Peekable;
use std::sync::Arc;

use tracing::trace;

use crate::index::IndexHunkIter;
use crate::monitor::Monitor;
use crate::*;

use super::entry::IndexEntry;

/// Stitch together any number of incomplete indexes to form a more-complete
/// index.
pub struct Stitch {
    /// The latest (and highest-ordered) apath we have already yielded.
    last_apath: Option<Apath>,

    archive: Archive,

    state: State,

    /// Filter entries according to these exclusions.
    exclude: Exclude,

    /// Only return entries within this directory.
    subtree: Apath,

    monitor: Arc<dyn Monitor>,
}

/// What state is a stitch iter in, and what should happen next?
#[allow(clippy::large_enum_variant)] // We only have typically one at any time
enum State {
    /// We've read to the end of a finished band, or to the earliest existing
    /// band, and there is no more content.
    Done,

    /// We have know the band to read and have not yet read it at all.
    BeforeBand(BandId),

    /// We have some index hunks from a band and can return them gradually.
    InBand {
        band_id: BandId,
        /// Temporarily buffered entries, read from the index files but not yet
        /// returned to the client. If this is empty, it's time to read the next
        /// hunk, or try the next band, or just the end.
        buffered_entries: Peekable<std::vec::IntoIter<IndexEntry>>,
        /// Hunks not yet returned from this band.
        index_hunks: IndexHunkIter,
    },

    /// We finished reading a band
    AfterBand(BandId),
}

impl Stitch {
    /// Return an iterator that reconstructs the most complete available index
    /// for a possibly-incomplete band.
    ///
    /// If the band is complete, this is simply the band's index.
    ///
    /// If it's incomplete, it stitches together the index by picking up at
    /// the same point in the previous band, continuing backwards recursively
    /// until either there are no more previous indexes, or a complete index
    /// is found.
    pub(crate) fn new(
        archive: &Archive,
        band_id: BandId,
        subtree: Apath,
        exclude: Exclude,
        monitor: Arc<dyn Monitor>,
    ) -> Stitch {
        Stitch {
            archive: archive.clone(),
            last_apath: None,
            state: State::BeforeBand(band_id),
            exclude,
            subtree,
            monitor,
        }
    }

    /// Construct a stitcher that will just return nothing.
    pub(crate) fn empty(archive: &Archive, monitor: Arc<dyn Monitor>) -> Stitch {
        Stitch {
            archive: archive.clone(),
            last_apath: None,
            state: State::Done,
            exclude: Exclude::nothing(),
            subtree: Apath::root(),
            monitor,
        }
    }
}

impl Iterator for Stitch {
    type Item = IndexEntry;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.state = match &mut self.state {
                State::Done => return None,
                State::InBand {
                    band_id,
                    index_hunks,
                    buffered_entries,
                } => {
                    if let Some(entry) = buffered_entries.next() {
                        // TODO: We could be smarter about skipping ahead if nothing
                        // in this page matches; or terminating early if we know
                        // nothing else in the index can be under this subtree.
                        if !self.subtree.is_prefix_of(&entry.apath)
                            || self.exclude.matches(&entry.apath)
                        {
                            continue;
                        } else {
                            return Some(entry);
                        }
                    } else if let Some(hunk) = index_hunks.next() {
                        if let Some(last_apath) = hunk.last().map(|entry| entry.apath.clone()) {
                            self.last_apath = Some(last_apath);
                        }
                        *buffered_entries = hunk.into_iter().peekable();
                        continue;
                    } else {
                        State::AfterBand(*band_id)
                    }
                }
                State::BeforeBand(band_id) => {
                    // Start reading this new index and skip forward until after last_apath
                    match Band::open(&self.archive, *band_id) {
                        Ok(band) => {
                            let mut index_hunks = band.index().iter_available_hunks();
                            if let Some(last) = &self.last_apath {
                                index_hunks = index_hunks.advance_to_after(last)
                            }
                            State::InBand {
                                band_id: *band_id,
                                index_hunks,
                                buffered_entries: Vec::new().into_iter().peekable(),
                            }
                        }
                        Err(err) => {
                            self.monitor.error(err);
                            State::AfterBand(*band_id)
                        }
                    }
                }
                State::AfterBand(band_id) => {
                    if self.archive.band_is_closed(*band_id).unwrap_or(false) {
                        trace!(?band_id, "band is closed; stitched iteration complete");
                        State::Done
                    } else if let Some(prev_band_id) =
                        previous_existing_band(&self.archive, *band_id)
                    {
                        trace!(?band_id, ?prev_band_id, "moving back to previous band");
                        State::BeforeBand(prev_band_id)
                    } else {
                        trace!(
                            ?band_id,
                            "no previous band to stitch; stitched iteration is complete"
                        );
                        State::Done
                    }
                }
            }
        }
    }
}

fn previous_existing_band(archive: &Archive, mut band_id: BandId) -> Option<BandId> {
    loop {
        // TODO: It might be faster to list the present bands, maybe when
        // constructing Stitch, and calculate from that, rather than walking
        // backwards one at a time...
        if let Some(prev_band_id) = band_id.previous() {
            band_id = prev_band_id;
            if archive.band_exists(band_id).unwrap_or(false) {
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
    use crate::counters::Counter;
    use crate::monitor::test::TestMonitor;
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

    fn simple_ls(archive: &Archive, band_id: BandId) -> String {
        let strs: Vec<String> = Stitch::new(
            archive,
            band_id,
            Apath::root(),
            Exclude::nothing(),
            TestMonitor::arc(),
        )
        .map(|entry| format!("{}:{}", &entry.apath, entry.target.unwrap()))
        .collect();
        strs.join(" ")
    }

    #[tokio::test]
    async fn stitch_index() -> Result<()> {
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

        let monitor = TestMonitor::arc();
        let band = Band::create(&af).await?;
        assert_eq!(band.id(), BandId::zero());
        let mut ib = band.index_writer(monitor.clone());
        ib.push_entry(symlink("/0", "b0"));
        ib.push_entry(symlink("/1", "b0"));
        ib.finish_hunk()?;
        ib.push_entry(symlink("/2", "b0"));
        // Flush this hunk but leave the band incomplete.
        let hunks = ib.finish()?;
        assert_eq!(hunks, 2);
        assert_eq!(
            monitor.get_counter(Counter::IndexWrites),
            2,
            "2 hunks were finished"
        );

        let monitor = TestMonitor::arc();
        let band = Band::create(&af).await?;
        assert_eq!(band.id().to_string(), "b0001");
        let mut ib = band.index_writer(monitor.clone());
        ib.push_entry(symlink("/0", "b1"));
        ib.push_entry(symlink("/1", "b1"));
        ib.finish_hunk()?;
        ib.push_entry(symlink("/2", "b1"));
        ib.push_entry(symlink("/3", "b1"));
        let hunks = ib.finish()?;
        assert_eq!(hunks, 2);
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 2);
        band.close(2)?;

        // b2
        let monitor = TestMonitor::arc();
        let band = Band::create(&af).await?;
        assert_eq!(band.id().to_string(), "b0002");
        let mut ib = band.index_writer(monitor.clone());
        ib.push_entry(symlink("/0", "b2"));
        ib.finish_hunk()?;
        ib.push_entry(symlink("/2", "b2"));
        // incomplete
        let hunks = ib.finish()?;
        assert_eq!(hunks, 2);
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 2);

        // b3
        let band = Band::create(&af).await?;
        assert_eq!(band.id().to_string(), "b0003");

        // b4
        let band = Band::create(&af).await?;
        assert_eq!(band.id().to_string(), "b0004");

        // b5
        let monitor = TestMonitor::arc();
        let band = Band::create(&af).await?;
        assert_eq!(band.id().to_string(), "b0005");
        let mut ib = band.index_writer(monitor.clone());
        ib.push_entry(symlink("/0", "b5"));
        ib.push_entry(symlink("/00", "b5"));
        let hunks = ib.finish()?;
        assert_eq!(hunks, 1);
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 1);
        // incomplete

        std::fs::remove_dir_all(af.path().join("b0003"))?;

        let archive = Archive::open_path(af.path())?;
        assert_eq!(simple_ls(&archive, BandId::new(&[0])), "/0:b0 /1:b0 /2:b0");

        assert_eq!(
            simple_ls(&archive, BandId::new(&[1])),
            "/0:b1 /1:b1 /2:b1 /3:b1"
        );

        assert_eq!(simple_ls(&archive, BandId::new(&[2])), "/0:b2 /2:b2 /3:b1");

        assert_eq!(simple_ls(&archive, BandId::new(&[4])), "/0:b2 /2:b2 /3:b1");

        assert_eq!(
            simple_ls(&archive, BandId::new(&[5])),
            "/0:b5 /00:b5 /2:b2 /3:b1"
        );

        Ok(())
    }

    /// Testing that the StitchedIndexHunks iterator does not loop forever on
    /// archives with at least one band but no completed bands.
    ///
    /// Reference: https://github.com/sourcefrog/conserve/pull/175
    #[tokio::test]
    async fn issue_175() {
        let tf = TreeFixture::new();
        tf.create_file("file_a");

        let af = ScratchArchive::new();
        backup(
            &af,
            tf.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .expect("backup should work");

        af.transport().remove_file("b0000/BANDTAIL").unwrap(); // band is now incomplete
        let band_ids = af.list_band_ids().await.expect("should list bands");

        let band_id = band_ids.first().expect("expected at least one band");

        let monitor = TestMonitor::arc();
        let mut entries = Stitch::new(
            &af,
            *band_id,
            Apath::root(),
            Exclude::nothing(),
            monitor.clone(),
        );
        // It should have two entries.
        assert_eq!(entries.next().unwrap().apath, "/");
        assert_eq!(entries.next().unwrap().apath, "/file_a");
        assert!(entries.next().is_none());
        assert!(entries.next().is_none());

        // Remove the band head. This band can not be opened anymore.
        // If accessed this should fail the test.
        // Note: When refactoring `.expect("Failed to open band")` this might needs refactoring as well.
        af.transport().remove_file("b0000/BANDHEAD").unwrap();

        // You can keep polling the iterator but it will keep returning none,
        // without trying to reopen the band.
        assert!(entries.next().is_none());
        assert!(entries.next().is_none());

        // It's not an error (at the moment) because a band with no head effectively doesn't exist.
        // (Maybe later the presence of a band directory with no head file should raise a warning.)
        let errors = monitor.take_errors();
        dbg!(&errors);
        assert_eq!(errors.len(), 0);
    }
}
