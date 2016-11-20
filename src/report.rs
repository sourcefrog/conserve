// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Accumulate statistics about a Conserve operation.
//!
//! A report includes counters of events, and also sizes for files.
//!
//! Sizes can be reported in both compressed and uncompressed form.

use std::cell;
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use std::time;
use std::time::{Duration};
use super::ui::UI;
use super::ui::terminal::TermUI;

static KNOWN_COUNTERS: &'static [&'static str] = &[
    "backup.dir",
    "backup.file",
    "backup.symlink",
    "backup.error.stat",
    "backup.skipped.unsupported_file_kind",
    "block.read",
    "block.read.corrupt",
    "block.read.misplaced",
    "block.write.already_present",
    "block.write",
    "index.read.hunks",
    "index.write.hunks",
    "restore.dir",
    "restore.file",
    "restore.symlink",
    "source.error.metadata",
    "source.selected",
    "source.skipped.unsupported_file_kind",
    "source.visited.directories",
];


static KNOWN_SIZES: &'static [&'static str] = &[
    "block.write",
    "index.write",
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
struct Inner {
    count: BTreeMap<&'static str, u64>,
    sizes: BTreeMap<&'static str, (u64, u64)>,
    durations: BTreeMap<&'static str, Duration>,
    start: time::Instant,
    ui: Option<TermUI>,
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
    inner: Rc<cell::RefCell<Inner>>,
}


impl Report {
    pub fn new() -> Report {
        let mut inner_count = BTreeMap::new();
        let mut inner_sizes = BTreeMap::new();
        let mut inner_durations: BTreeMap<&'static str, Duration> = BTreeMap::new();
        for counter_name in KNOWN_COUNTERS {
            inner_count.insert(*counter_name, 0);
        };
        for counter_name in KNOWN_SIZES {
            inner_sizes.insert(*counter_name, (0, 0));
        };
        for name in KNOWN_DURATIONS {
            inner_durations.insert(name, Duration::new(0, 0));
        };
        let inner = Inner {
            count: inner_count,
            sizes: inner_sizes,
            durations: inner_durations,
            start: time::Instant::now(),
            ui: TermUI::new(),
        };
        Report {
            inner: Rc::new(cell::RefCell::new(inner)),
        }
    }

    fn mut_inner(&self) -> cell::RefMut<Inner> {
        self.inner.borrow_mut()
    }

    /// Increment a counter by a given amount.
    ///
    /// The name must be a static string.  Counters implicitly start at 0.
    pub fn increment(&self, counter_name: &'static str, delta: u64) {
        // Entries are created from the list of known names when this is constructed.
        if let Some(mut c) = self.mut_inner().count.get_mut(counter_name) {
            *c += delta;
        } else {
            panic!("unregistered counter {:?}", counter_name);
        }
        if let Some(ref ui) = self.inner.borrow().ui {
            ui.show_progress(self);
        }
    }

    pub fn increment_size(&self, counter_name: &str, uncompressed_bytes: u64,
        compressed_bytes: u64) {
        let mut inner = self.mut_inner();
        let mut e = inner.sizes.get_mut(counter_name).expect("unregistered size counter");
        e.0 += uncompressed_bytes;
        e.1 += compressed_bytes;
    }

    pub fn increment_duration(&self, name: &str, duration: Duration) {
        *self.mut_inner().durations
            .get_mut(name).expect("undefined duration counter")
            += duration;
    }

    /// Return the value of a counter.  A counter that has not yet been updated is 0.
    pub fn get_count(&self, counter_name: &str) -> u64 {
        *self.inner.borrow().count.get(counter_name).expect("unknown counter")
    }

    /// Get size of data processed.
    ///
    /// For any size-counter name, returns a pair of (compressed, uncompressed) sizes,
    /// in bytes.
    pub fn get_size(&self, counter_name: &str) -> (u64, u64) {
        *self.inner.borrow().sizes.get(counter_name).expect("unknown size counter")
    }

    pub fn get_duration(&self, name: &str) -> Duration {
        *self.inner.borrow().durations.get(name).expect("unknown duration name")
    }

    pub fn elapsed_time(&self) -> Duration {
        self.inner.borrow().start.elapsed()
    }

    /// Merge the contents of `from_report` into `self`.
    pub fn merge_from(&self, from_report: &Report) {
        let from_inner = from_report.mut_inner();
        for (name, value) in &from_inner.count {
            self.increment(name, *value);
        }
        for (name, &(uncompressed, compressed)) in &from_inner.sizes {
            self.increment_size(name, uncompressed, compressed);
        }
        for (name, duration) in &from_inner.durations {
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
        let inner = self.mut_inner();
        for (key, value) in &inner.count {
            if *value > 0 {
                try!(write!(f, "  {:<50}{:>10}\n", *key, *value));
            }
        }
        try!(write!(f, "Bytes (before and after compression):\n"));
        for (key, &(uncompressed_bytes, compressed_bytes)) in &inner.sizes {
            if uncompressed_bytes > 0 {
                let compression_pct =
                    100 - ((100 * compressed_bytes) / uncompressed_bytes);
                try!(write!(f, "  {:<50} {:>9} {:>9} {:>9}%\n", *key,
                    uncompressed_bytes, compressed_bytes, compression_pct));
            }
        }
        try!(write!(f, "Durations (seconds):\n"));
        for (key, &dur) in &inner.durations {
            let millis = dur.subsec_nanos() / 1000000;
            let secs = dur.as_secs();
            if millis > 0 || secs > 0 {
                try!(write!(f, "  {:<40} {:>15}.{:>03}\n", key, secs, millis));
            }
        }
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::Report;

    #[test]
    pub fn count() {
        let r = Report::new();
        assert_eq!(r.get_count("block.read"), 0);
        r.increment("block.read", 1);
        assert_eq!(r.get_count("block.read"), 1);
        r.increment("block.read", 10);
        assert_eq!(r.get_count("block.read"), 11);
    }

    #[test]
    pub fn merge_reports() {
        let r1 = Report::new();
        let r2 = Report::new();
        r1.increment("block.read", 1);
        r1.increment("block.read.corrupt", 2);
        r2.increment("block.write", 1);
        r2.increment("block.read.corrupt", 10);
        r2.increment_size("block.write", 300, 100);
        r2.increment_duration("test", Duration::new(5, 0));
        r1.merge_from(&r2);
        assert_eq!(r1.get_count("block.read"), 1);
        assert_eq!(r1.get_count("block.read.corrupt"), 12);
        assert_eq!(r1.get_count("block.write"), 1);
        assert_eq!(r1.get_size("block.write"), (300, 100));
        assert_eq!(r1.get_duration("test"), Duration::new(5, 0));
    }

    #[test]
    pub fn display() {
        let r1 = Report::new();
        r1.increment("block.read", 10);
        r1.increment("block.write", 5);
        r1.increment_size("block.write", 300, 100);
        r1.increment_duration("test", Duration::new(42, 479760000));

        let formatted = format!("{}", r1);
        assert_eq!(formatted, "\
Counts:
  block.read                                                10
  block.write                                                5
Bytes (before and after compression):
  block.write                                              300       100        67%
Durations (seconds):
  test                                                  42.479
");
    }
}
