// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Accumulate statistics about a Conserve operation.
//!
//! A report includes counters of events, and also sizes for files.
//!
//! Sizes can be reported in both compressed and uncompressed form.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard};
use std::time;
use std::time::{Duration};

use log;

use super::ui::UI;
use super::ui::terminal::TermUI;
use super::ui::plain::PlainUI;

static KNOWN_COUNTERS: &'static [&'static str] = &[
    "dir",
    "file",
    "symlink",
    "backup.error.stat",
    "block",
    "block.corrupt",
    "block.misplaced",
    "block.already_present",
    "index.hunk",
    "source.error.metadata",
    "source.selected",
    "skipped.unsupported_file_kind",
    "source.visited.directories",
];


static KNOWN_SIZES: &'static [&'static str] = &[
    "block",
    "index",
];


static KNOWN_DURATIONS: &'static [&'static str] = &[
    "block.compress",
    "block.hash",
    "index.compress",
    "index.encode",
    "index.parse",
    "index.read",
    "source.read",
    "sync",
    "test",
];


/// Holds the actual counters, in an inner object that can be referenced by
/// multiple Report values.
pub struct Counts {
    count: BTreeMap<&'static str, u64>,
    sizes: BTreeMap<&'static str, (u64, u64)>,
    durations: BTreeMap<&'static str, Duration>,
    start: time::Instant,
}


/// A Report is notified of problems or non-problematic events that occur while Conserve is
/// running.
///
/// A Report holds counters, identified by a name.  The name must be in `KNOWN_COUNTERS`.
///
/// A Report is internally mutable, so a single instance can be shared by multiple objects
/// or scopes (on the same thread) who all append to it.
///
/// Cloning a Report makes another reference to the same underlying counters.
#[derive(Clone)]
pub struct Report {
    counts: Arc<Mutex<Counts>>,
    ui: Arc<Mutex<Box<UI + Send>>>,
}


impl Report {
    /// Default constructor with plain text UI.
    pub fn new() -> Report {
        Report::with_ui(Box::new(PlainUI::new()))
    }

    /// Make a new report viewed by a given UI.
    pub fn with_ui(ui_box: Box<UI + Send>) -> Report {
        Report {
            counts: Arc::new(Mutex::new(Counts::new())),
            ui: Arc::new(Mutex::new(ui_box)),
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
        if let Some(mut c) = self.mut_counts().count.get_mut(counter_name) {
            *c += delta;
        } else {
            panic!("unregistered counter {:?}", counter_name);
        }
        self.ui.lock().unwrap().show_progress(&*self.borrow_counts());
    }

    pub fn increment_size(&self, counter_name: &str, uncompressed_bytes: u64, compressed_bytes: u64) {
        let mut counts = self.mut_counts();
        let mut e = counts.sizes.get_mut(counter_name).expect("unregistered size counter");
        e.0 += uncompressed_bytes;
        e.1 += compressed_bytes;
    }

    pub fn increment_duration(&self, name: &str, duration: Duration) {
        *self.mut_counts().durations
            .get_mut(name).expect("undefined duration counter")
            += duration;
    }

    /// Merge the contents of `from_report` into `self`.
    pub fn merge_from(&self, from_report: &Report) {
        let from_counts = from_report.mut_counts();
        for (name, value) in &from_counts.count {
            self.increment(name, *value);
        }
        for (name, &(uncompressed, compressed)) in &from_counts.sizes {
            self.increment_size(name, uncompressed, compressed);
        }
        for (name, duration) in &from_counts.durations {
            self.increment_duration(name, *duration);
        }
    }

    pub fn measure_duration<T, F>(&self, duration_name: &str, mut closure: F) -> T
        where F: FnMut() -> T {
        let start = time::Instant::now();
        let result = closure();
        self.increment_duration(duration_name, start.elapsed());
        result
    }
}


impl Display for Report {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        try!(write!(f, "Counts:\n"));
        let counts = self.mut_counts();
        for (key, value) in &counts.count {
            if *value > 0 {
                try!(write!(f, "  {:<40} {:>9}\n", *key, *value));
            }
        }
        try!(write!(f, "Bytes (before and after compression):\n"));
        for (key, &(uncompressed_bytes, compressed_bytes)) in &counts.sizes {
            if uncompressed_bytes > 0 {
                let compression_pct =
                    100 - ((100 * compressed_bytes) / uncompressed_bytes);
                try!(write!(f, "  {:<40} {:>9} {:>9} {:>9}%\n", *key,
                    uncompressed_bytes, compressed_bytes, compression_pct));
            }
        }
        try!(write!(f, "Durations (seconds):\n"));
        for (key, &dur) in &counts.durations {
            let millis = dur.subsec_nanos() / 1000000;
            let secs = dur.as_secs();
            if millis > 0 || secs > 0 {
                try!(write!(f, "  {:<40} {:>5}.{:>03}\n", key, secs, millis));
            }
        }
        Ok(())
    }
}


impl log::Log for Report {
    fn enabled(&self, _metadata: &log::LogMetadata) -> bool {
        true
    }

    fn log(&self, record: &log::LogRecord) {
        self.ui.lock().unwrap().log(record);
    }
}


impl Counts {
    fn new() -> Counts {
        let mut inner_count = BTreeMap::new();
        for counter_name in KNOWN_COUNTERS {
            inner_count.insert(*counter_name, 0);
        };
        let mut inner_sizes = BTreeMap::new();
        for counter_name in KNOWN_SIZES {
            inner_sizes.insert(*counter_name, (0, 0));
        };
        let mut inner_durations: BTreeMap<&'static str, Duration> = BTreeMap::new();
        for name in KNOWN_DURATIONS {
            inner_durations.insert(name, Duration::new(0, 0));
        };
        Counts {
            count: inner_count,
            sizes: inner_sizes,
            durations: inner_durations,
            start: time::Instant::now(),
        }
    }

    pub fn get_duration(&self, name: &str) -> Duration {
        *self.durations.get(name).unwrap_or_else(
            || panic!("unknown duration {:?}", name))
    }

    /// Return the value of a counter.  A counter that has not yet been updated is 0.
    pub fn get_count(&self, counter_name: &str) -> u64 {
        *self.count.get(counter_name).unwrap_or_else(
            || panic!("unknown counter {:?}", counter_name))
    }

    /// Get size of data processed.
    ///
    /// For any size-counter name, returns a pair of (compressed, uncompressed) sizes,
    /// in bytes.
    pub fn get_size(&self, counter_name: &str) -> (u64, u64) {
        *self.sizes.get(counter_name).unwrap_or_else(
            || panic!("unknown counter {:?}", counter_name))
    }

    pub fn elapsed_time(&self) -> Duration {
        self.start.elapsed()
    }
}


#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::Report;

    #[test]
    pub fn count() {
        let r = Report::new();
        assert_eq!(r.borrow_counts().get_count("block"), 0);
        r.increment("block", 1);
        assert_eq!(r.borrow_counts().get_count("block"), 1);
        r.increment("block", 10);
        assert_eq!(r.borrow_counts().get_count("block"), 11);
    }

    #[test]
    pub fn merge_reports() {
        let r1 = Report::new();
        let r2 = Report::new();
        r1.increment("block", 1);
        r1.increment("block.corrupt", 2);
        r2.increment("block", 1);
        r2.increment("block.corrupt", 10);
        r2.increment_size("block", 300, 100);
        r2.increment_duration("test", Duration::new(5, 0));
        r1.merge_from(&r2);
        let cs = r1.borrow_counts();
        assert_eq!(cs.get_count("block"), 2);
        assert_eq!(cs.get_count("block.corrupt"), 12);
        assert_eq!(cs.get_size("block"), (300, 100));
        assert_eq!(cs.get_duration("test"), Duration::new(5, 0));
    }

    #[test]
    pub fn display() {
        let r1 = Report::new();
        r1.increment("block", 10);
        r1.increment("block", 5);
        r1.increment_size("block", 300, 100);
        r1.increment_duration("test", Duration::new(42, 479760000));

        let formatted = format!("{}", r1);
        assert_eq!(formatted, "\
Counts:
  block                                           15
Bytes (before and after compression):
  block                                          300       100        67%
Durations (seconds):
  test                                        42.479
");
    }
}
