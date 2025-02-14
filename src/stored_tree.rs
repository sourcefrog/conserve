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

use std::sync::Arc;

use crate::counters::Counter;
use crate::index::stitch::Stitch;
use crate::monitor::Monitor;
use crate::tree::TreeSize;
use crate::*;

/// Read index and file contents for a version stored in the archive.
#[derive(Debug)]
pub struct StoredTree {
    pub(crate) band: Band,
    pub(crate) archive: Archive,
}

impl StoredTree {
    pub(crate) fn open(archive: &Archive, band_id: BandId) -> Result<StoredTree> {
        Ok(StoredTree {
            band: Band::open(archive, band_id)?,
            archive: archive.clone(),
        })
    }

    pub fn band(&self) -> &Band {
        &self.band
    }

    pub fn is_closed(&self) -> Result<bool> {
        self.band.is_closed()
    }

    pub fn size(&self, exclude: Exclude, monitor: Arc<dyn Monitor>) -> Result<TreeSize> {
        let mut file_bytes = 0u64;
        let task = monitor.start_task("Measure tree".to_string());
        for e in self.iter_entries(Apath::from("/"), exclude, monitor.clone())? {
            // While just measuring size, ignore directories/files we can't stat.
            if let Some(bytes) = e.size() {
                monitor.count(Counter::Files, 1);
                monitor.count(Counter::FileBytes, bytes as usize);
                file_bytes += bytes;
                task.increment(bytes as usize);
            }
        }
        Ok(TreeSize { file_bytes })
    }

    /// Return an iter of index entries in this stored tree.
    // TODO: Should perhaps return a sequence of results so that the caller has the
    // option to handle errors or continue.
    pub fn iter_entries(
        &self,
        subtree: Apath,
        exclude: Exclude,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Stitch> {
        // TODO: Pass in this band so that we don't need to reopen it.
        Ok(Stitch::new(
            &self.archive,
            self.band.id(),
            subtree,
            exclude,
            monitor,
        ))
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::monitor::test::TestMonitor;

    use super::super::test_fixtures::*;
    use super::super::*;

    #[test]
    pub fn open_stored_tree() {
        let af = ScratchArchive::new();
        af.store_two_versions();

        let last_band_id = af.last_band_id().unwrap().unwrap();
        let st = af.open_stored_tree(BandSelectionPolicy::Latest).unwrap();

        assert_eq!(st.band().id(), last_band_id);

        let monitor = TestMonitor::arc();
        let names: Vec<String> = st
            .iter_entries(Apath::root(), Exclude::nothing(), monitor.clone())
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
        assert_eq!(
            af.open_stored_tree(BandSelectionPolicy::Latest)
                .unwrap_err()
                .to_string(),
            "Archive is empty"
        );
    }

    #[test]
    fn iter_entries() {
        let archive = Archive::open_path(Path::new("testdata/archive/minimal/v0.6.3/")).unwrap();
        let st = archive
            .open_stored_tree(BandSelectionPolicy::Latest)
            .unwrap();

        let monitor = TestMonitor::arc();
        let names: Vec<String> = st
            .iter_entries("/subdir".into(), Exclude::nothing(), monitor.clone())
            .unwrap()
            .map(|entry| entry.apath.into())
            .collect();

        assert_eq!(names.as_slice(), ["/subdir", "/subdir/subfile"]);
    }
}
