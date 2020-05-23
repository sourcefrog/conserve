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

use super::ui::{PlainUI, UI};
use super::*;

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
    "block.read",
    "block.write",
    "block.corrupt",
    "block.misplaced",
    "block.already_present",
    "index.hunk",
    "source.error.metadata",
    "source.selected",
    "skipped.unsupported_file_kind",
    "source.visited.directories",
    "skipped.excluded.directories",
    "skipped.excluded.files",
    "skipped.excluded.symlinks",
    "skipped.excluded.unknown",
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
    start: Instant,

    /// Most recently started filename.
    latest_filename: String,

    /// General phase of work.
    pub phase: String,

    /// Total estimated work to be done (task-specific units).
    pub total_work: u64,

    /// Amount of work done so far, to indicate percentage completion.
    pub done_work: u64,

    /// Number of errors observed.
    pub error_count: u64,
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
    ui: Arc<Mutex<Box<dyn UI + Send>>>,
    print_filenames: bool,
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
        Report::with_ui(Box::new(PlainUI::new()))
    }

    /// Make a new report viewed by a given UI.
    pub fn with_ui(ui_box: Box<dyn UI + Send>) -> Report {
        Report {
            counts: Arc::new(Mutex::new(Counts::new())),
            ui: Arc::new(Mutex::new(ui_box)),
            print_filenames: false,
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
        self.show_progress();
    }

    /// Update the progress bars for the current counts, etc.
    fn show_progress(&self) {
        // If another thread is drawing the UI, don't wait, just skip it.
        if let Ok(mut ui) = self.ui.try_lock() {
            ui.show_progress(self);
        }
    }

    pub fn get_count(&self, counter_name: &str) -> u64 {
        self.borrow_counts().get_count(counter_name)
    }

    /// Report that processing started for a given entry.
    pub fn start_entry(&self, apath: &Apath) {
        self.mut_counts().latest_filename = apath.to_string();
    }

    /// Briefly describe the phase of work.
    pub fn set_phase<S: Into<String>>(&self, phase: S) {
        self.mut_counts().phase = phase.into();
    }

    pub fn clear_phase(&self) {
        self.set_phase("");
    }

    pub fn println(&self, s: &str) {
        self.ui.lock().unwrap().println(s)
    }

    /// Report that a problem occurred.
    ///
    /// Later this might also count or summarize them.
    pub fn problem(&self, s: &str) {
        // TODO: Convert callers to calling Report::warning passing a structured
        // error.
        // <https://github.com/sourcefrog/conserve/issues/72>.
        self.mut_counts().error_count += 1;
        self.ui.lock().unwrap().problem(s).unwrap();
    }

    /// Report that a non-fatal error occurred.
    ///
    /// The program will continue.
    pub fn show_error(&self, e: &dyn std::error::Error) {
        self.mut_counts().error_count += 1;
        let mut ui = self.ui.lock().unwrap();
        ui.problem(&e.to_string()).unwrap();
        let mut ce = e;
        while let Some(c) = ce.source() {
            ui.problem(&format!("  caused by: {}", c)).unwrap();
            ce = c;
        }
    }

    pub fn finish(&self) {
        self.ui.lock().unwrap().finish()
    }

    /// Set the total expected work (in bytes); this also resets the amount of work done.
    pub fn set_total_work(&self, w: u64) {
        let mut c = self.mut_counts();
        c.total_work = w;
        c.done_work = 0;
    }

    pub fn increment_work(&self, w: u64) {
        self.mut_counts().done_work += w;
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
            latest_filename: String::new(),
            phase: String::new(),
            total_work: 0,
            done_work: 0,
            error_count: 0,
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

    pub fn get_latest_filename(&self) -> &str {
        &self.latest_filename
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn count() {
        let r = Report::new();
        assert_eq!(r.borrow_counts().get_count("block.read"), 0);
        r.increment("block.read", 1);
        assert_eq!(r.borrow_counts().get_count("block.read"), 1);
        r.increment("block.read", 10);
        assert_eq!(r.borrow_counts().get_count("block.read"), 11);
    }
}
