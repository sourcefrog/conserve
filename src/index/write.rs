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

//! The index lists all the files in a backup, sorted in apath order.

use std::sync::Arc;

use tracing::trace;

use crate::compress::snappy::Compressor;
use crate::counters::Counter;
use crate::monitor::Monitor;
use crate::transport::{Transport, WriteMode};
use crate::*;

use super::{hunk_relpath, subdir_relpath, HUNKS_PER_SUBDIR};

/// Write out index hunks.
///
/// This class is responsible for: remembering the hunk number, and checking that the
/// hunks preserve apath order.
pub struct IndexWriter {
    /// The `i` directory within the band where all files for this index are written.
    transport: Transport,

    /// Currently queued entries to be written out, in arbitrary order.
    entries: Vec<IndexEntry>,

    /// Index hunk number, starting at 0.
    sequence: u32,

    /// Number of hunks actually written.
    pub(super) hunks_written: usize,

    /// The last filename from the previous hunk, to enforce ordering. At the
    /// start of the first hunk this is empty; at the start of a later hunk it's
    /// the last path from the previous hunk.
    check_order: apath::DebugCheckOrder,

    compressor: Compressor,

    monitor: Arc<dyn Monitor>,
}

/// Accumulate and write out index entries into files in an index directory.
impl IndexWriter {
    /// Make a new builder that will write files into the given directory.
    pub fn new(transport: Transport, monitor: Arc<dyn Monitor>) -> IndexWriter {
        IndexWriter {
            transport,
            entries: Vec::new(),
            sequence: 0,
            hunks_written: 0,
            check_order: apath::DebugCheckOrder::new(),
            compressor: Compressor::new(),
            monitor,
        }
    }

    /// Finish the last hunk of this index, and return the stats.
    pub fn finish(mut self) -> Result<usize> {
        self.finish_hunk()?;
        Ok(self.hunks_written)
    }

    /// Return the number of queued up pending entries.
    pub fn pending_entries(&self) -> usize {
        self.entries.len()
    }

    /// Write new index entries.
    ///
    /// Entries within one hunk may be added in arbitrary order, but they must all
    /// sort after previously-written content.
    ///
    /// The new entry must sort after everything already written to the index.
    pub(crate) fn push_entry(&mut self, entry: IndexEntry) {
        self.entries.push(entry);
    }

    pub(crate) fn append_entries(&mut self, entries: &mut Vec<IndexEntry>) {
        // NB: This can exceed the maximum if many entries are added at once.
        self.entries.append(entries);
    }

    /// Finish this hunk of the index.
    ///
    /// This writes all the currently queued entries into a new index file
    /// in the band directory, and then clears the buffer to start receiving
    /// entries for the next hunk.
    pub fn finish_hunk(&mut self) -> Result<()> {
        if self.entries.is_empty() {
            // TODO: Maybe assert that it's not empty?
            return Ok(());
        }
        trace!(
            hunk_index = self.sequence,
            n_entries = self.entries.len(),
            "Finish hunk"
        );
        self.entries.sort_unstable_by(|a, b| {
            debug_assert!(a.apath != b.apath);
            a.apath.cmp(&b.apath)
        });
        self.check_order.check(&self.entries[0].apath);
        if self.entries.len() > 1 {
            self.check_order.check(&self.entries.last().unwrap().apath);
        }
        let relpath = hunk_relpath(self.sequence);
        let json = serde_json::to_vec(&self.entries)?;
        if (self.sequence % HUNKS_PER_SUBDIR) == 0 {
            self.transport.create_dir(&subdir_relpath(self.sequence))?;
        }
        let compressed_bytes = self.compressor.compress(&json)?;
        self.transport
            .write(&relpath, &compressed_bytes, WriteMode::CreateNew)?;
        self.hunks_written += 1;
        self.monitor.count(Counter::IndexWrites, 1);
        self.monitor
            .count(Counter::IndexWriteCompressedBytes, compressed_bytes.len());
        self.monitor
            .count(Counter::IndexWriteUncompressedBytes, json.len());
        self.entries.clear(); // Ready for the next hunk.
        self.sequence += 1;
        Ok(())
    }
}
