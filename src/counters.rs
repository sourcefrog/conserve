// Copyright 2023 Martin Pool

//! Track counters of the number of files, bytes, blocks, etc, processed.
//!
//! Library code sets counters through the [Monitor] interface.

#![warn(missing_docs)]

use std::fmt::{self, Debug};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

use itertools::Itertools;
use strum::{EnumCount, IntoEnumIterator};
use strum_macros::{EnumCount, EnumIter};

/// Counters of events or bytes.
#[derive(Debug, Clone, Copy, Eq, PartialEq, EnumCount, EnumIter)]
pub enum Counter {
    /// Number of files processed (restored, backed up, etc).
    ///
    /// Includes files that are unchanged, but not files that are excluded.
    Files,
    /// Total bytes in files processed.
    FileBytes,
    /// Number of directories processed.
    Dirs,
    /// Number of symlinks processed.
    Symlinks,
    /// Number of entries (files etc) that are unchanged from the basis backup.
    EntriesUnchanged,
    /// Number of entries changed since the basis backup.
    EntriesChanged,
    /// Number of entries added since the basis backup.
    EntriesAdded,
    /// Number of entries deleted relative to the basis backup.
    EntriesDeleted,
    /// Number of files with length zero.
    EmptyFiles,
    /// Number of small files packed into combined blocks.
    SmallFiles,
    /// Number of files that used a single block: not combined but not broken into multiple blocks.
    SingleBlockFiles,
    /// Number of files broken into multiple blocks.
    MultiBlockFiles,
    /// Number of blocks that matched a hash-addressed block that's already present.
    DeduplicatedBlocks,
    /// Total bytes in deduplicated blocks.
    DeduplicatedBlockBytes,
    /// Blocks written.
    BlockWrites,
    /// Total uncompressed bytes in blocks written out.
    BlockWriteUncompressedBytes,
    /// Total compressed bytes in blocks written out.
    BlockWriteCompressedBytes,
    /// Found the content of a block in memory.
    BlockContentCacheHit,
    /// Failed to find a block in memory.
    BlockContentCacheMiss,
}

/// Counter values, identified by a [Counter].
#[derive(Default)]
pub struct Counters {
    counters: [AtomicUsize; Counter::COUNT],
}

impl Counters {
    /// Increase the value for a given counter by an amount.
    pub fn count(&self, counter: Counter, increment: usize) {
        self.counters[counter as usize].fetch_add(increment, Relaxed);
    }

    /// Set the absolute value of a counter.
    pub fn set(&self, counter: Counter, value: usize) {
        self.counters[counter as usize].store(value, Relaxed);
    }

    /// Get the current value of a counter.
    pub fn get(&self, counter: Counter) -> usize {
        self.counters[counter as usize].load(Relaxed)
    }

    /// Return an iterator over counter, value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (Counter, usize)> {
        Counter::iter()
            .map(move |c| (c, self.counters[c as usize].load(Relaxed)))
            .collect_vec()
            .into_iter()
    }
}

impl Debug for Counters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Counters");
        for i in Counter::iter() {
            s.field(
                &format!("{:?}", i),
                &self.counters[i as usize].load(Relaxed),
            );
        }
        s.finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn simple_counts() {
        let counters = Counters::default();
        counters.count(Counter::Files, 1);
        counters.count(Counter::Files, 2);
        counters.set(Counter::FileBytes, 100);
        assert_eq!(counters.get(Counter::Files), 3);
        assert_eq!(counters.get(Counter::Dirs), 0);
        assert_eq!(counters.get(Counter::FileBytes), 100);
    }

    #[test]
    fn iter_counters() {
        let counters = Counters::default();
        counters.count(Counter::Files, 2);
        dbg!(&counters);

        counters.iter().for_each(|(c, v)| {
            assert_eq!(counters.get(c), v);
        });
        assert_eq!(counters.iter().count(), Counter::COUNT);
        assert!(counters
            .iter()
            .all(|(c, v)| (c == Counter::Files) == (v == 2)));
    }

    #[test]
    fn debug_form() {
        let counters = Counters::default();
        counters.count(Counter::Files, 2);
        let d = format!("{counters:#?}");
        println!("{}", d);
        assert!(d.contains("Files: 2"));
        assert!(d.contains("Dirs: 0"));
    }
}
