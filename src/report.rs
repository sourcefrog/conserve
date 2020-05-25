// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Accumulate statistics about a Conserve operation.
//!
//! A report includes counters of events.
//!
//! By convention in this library when a Report is explicitly provided, it's the last parameter.
//!
//! When possible a Report is inherited from an `Archive` into the objects created from it.

use std::collections::BTreeMap;
use std::ops::AddAssign;
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[rustfmt::skip]
static KNOWN_COUNTERS: &[&str] = &[
    "dir",
    "file",
    "file.empty",
    "file.medium",
    "file.large",
    "file.unchanged",
    "symlink",
    "backup.error.stat",
    "block.write",
    "block.already_present",
    "skipped.unsupported_file_kind",
];

/// Describes sizes of data read or written, with both the
/// compressed and uncompressed size.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sizes {
    pub compressed: u64,
    pub uncompressed: u64,
}

/// Holds the actual counters, in an inner object that can be referenced by
/// multiple Report values.
#[derive(Debug)]
pub struct Counts {
    count: BTreeMap<&'static str, u64>,
    pub start: Instant,
}

/// A Report is notified of problems or non-problematic events that occur while Conserve is
/// running.
///
/// A Report holds counters, identified by a name.  The name must be in `KNOWN_COUNTERS`.
///
/// A Report is internally mutable, so a single instance can be shared by multiple objects
/// or scopes (on the same thread) who all append to it.
///
/// Cloning a Report makes a shared reference to the same underlying counters.
#[derive(Clone, Debug)]
pub struct Report {
    counts: Arc<Mutex<Counts>>,
}

/// Trees and Archives have a Report as general context for operations on them.
pub trait HasReport {
    fn report(&self) -> &Report;
}

impl AddAssign for Sizes {
    fn add_assign(&mut self, other: Sizes) {
        self.compressed += other.compressed;
        self.uncompressed += other.uncompressed;
    }
}

impl<'a> AddAssign<&'a Sizes> for Sizes {
    fn add_assign(&mut self, other: &'a Sizes) {
        self.compressed += other.compressed;
        self.uncompressed += other.uncompressed;
    }
}

impl Report {
    /// Default constructor with plain text UI.
    pub fn new() -> Report {
        Report {
            counts: Arc::new(Mutex::new(Counts::new())),
        }
    }

    fn mut_counts(&self) -> MutexGuard<Counts> {
        self.counts.lock().unwrap()
    }

    /// Borrow (read-only) counters inside this report.
    pub fn borrow_counts(&self) -> MutexGuard<Counts> {
        self.counts.lock().unwrap()
    }

    /// Increment a counter by a given amount.
    ///
    /// The name must be a static string.  Counters implicitly start at 0.
    pub fn increment(&self, counter_name: &'static str, delta: u64) {
        // Entries are created from the list of known names when this is constructed.
        if let Some(c) = self.mut_counts().count.get_mut(counter_name) {
            *c += delta;
        } else {
            panic!("unregistered counter {:?}", counter_name);
        }
    }

    pub fn get_count(&self, counter_name: &str) -> u64 {
        self.borrow_counts().get_count(counter_name)
    }
}

impl Default for Report {
    fn default() -> Self {
        Self::new()
    }
}

impl Counts {
    fn new() -> Counts {
        let mut count = BTreeMap::new();
        for counter_name in KNOWN_COUNTERS {
            count.insert(*counter_name, 0);
        }
        Counts {
            count,
            start: Instant::now(),
        }
    }

    /// Return the value of a counter.  A counter that has not yet been updated is 0.
    pub fn get_count(&self, counter_name: &str) -> u64 {
        *self
            .count
            .get(counter_name)
            .unwrap_or_else(|| panic!("unknown counter {:?}", counter_name))
    }

    pub fn elapsed_time(&self) -> Duration {
        self.start.elapsed()
    }
}
