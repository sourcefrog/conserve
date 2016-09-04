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
use std::time::{Duration};

static KNOWN_COUNTERS: &'static [&'static str] = &[
    "backup.file.count",
    "backup.error.stat",
    "backup.skipped.unsupported_file_kind",
    "block.read.count",
    "block.read.corrupt",
    "block.read.misplaced",
    "block.write.already_present",
    "block.write.count",
    "index.read.hunks",
    "index.write.hunks",
    "source.error.metadata",
    "source.selected.count",
    "source.skipped.unsupported_file_kind",
    "source.visited.directories.count",
];


static KNOWN_SIZES: &'static [&'static str] = &[
    "block.write",
    "index.write",
];


static KNOWN_DURATIONS: &'static [&'static str] = &[
    "index.parse",
    "index.read",
    "source.read",
    "sync",
    "test",
];

/// A Report is notified of problems or non-problematic events that occur while Conserve is
/// running.
///
/// A Report holds counters, identified by a name.  The name must be in `KNOWN_COUNTERS`.
#[derive(Clone, Debug, Default)]
pub struct Report {
    count: BTreeMap<&'static str, u64>,
    sizes: BTreeMap<&'static str, (u64, u64)>,
    durations: BTreeMap<&'static str, Duration>,
}

impl Report {
    pub fn new() -> Report {
        let mut new = Report {
            count: BTreeMap::new(),
            sizes: BTreeMap::new(),
            durations: BTreeMap::new(),
        };
        for counter_name in KNOWN_COUNTERS {
            new.count.insert(*counter_name, 0);
        }
        for counter_name in KNOWN_SIZES {
            new.sizes.insert(*counter_name, (0, 0));
        }
        for name in KNOWN_DURATIONS {
            new.durations.insert(name, Duration::new(0, 0));
        }
        new
    }

    /// Increment a counter by a given amount.
    ///
    /// The name must be a static string.  Counters implicitly start at 0.
    pub fn increment(self: &mut Report, counter_name: &'static str, delta: u64) {
        // Entries are created from the list of known names when this is constructed.
        if let Some(mut c) = self.count.get_mut(counter_name) {
            *c += delta;
        } else {
            panic!("unregistered counter {:?}", counter_name);
        }
    }

    pub fn increment_size(&mut self, counter_name: &str, uncompressed_bytes: u64,
        compressed_bytes: u64) {
        let mut e = self.sizes.get_mut(counter_name).expect("unregistered size counter");
        e.0 += uncompressed_bytes;
        e.1 += compressed_bytes;
    }

    pub fn increment_duration(&mut self, name: &str, duration: Duration) {
        let mut e = self.durations.get_mut(name).expect("undefined duration counter");
        *e += duration;
    }

    /// Return the value of a counter.  A counter that has not yet been updated is 0.
    #[allow(unused)]
    pub fn get_count(&self, counter_name: &str) -> u64 {
        *self.count.get(counter_name).unwrap_or(&0)
    }

    pub fn get_size(&self, counter_name: &str) -> (u64, u64) {
        *self.sizes.get(counter_name).expect("unknown size counter")
    }

    pub fn get_duration(&self, name: &str) -> Duration {
        *self.durations.get(name).expect("unknown duration name")
    }

    /// Merge the contents of `from_report` into `self`.
    pub fn merge_from(self: &mut Report, from_report: &Report) {
        for (name, value) in &from_report.count {
            self.increment(name, *value);
        }
        for (name, &(uncompressed, compressed)) in &from_report.sizes {
            self.increment_size(name, uncompressed, compressed);
        }
        for (name, duration) in &from_report.durations {
            self.increment_duration(name, *duration);
        }
    }
}


impl Display for Report {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        try!(write!(f, "Counts:\n"));
        for (key, value) in &self.count {
            if *value > 0 {
                try!(write!(f, "  {:<50}{:>10}\n", *key, *value));
            }
        }
        try!(write!(f, "Bytes (before and after compression):\n"));
        for (key, &(uncompressed_bytes, compressed_bytes)) in &self.sizes {
            if uncompressed_bytes > 0 {
                let compression_pct =
                    100 - ((100 * compressed_bytes) / uncompressed_bytes);
                try!(write!(f, "  {:<50} {:>9} {:>9} {:>9}%\n", *key,
                    uncompressed_bytes, compressed_bytes, compression_pct));
            }
        }
        try!(write!(f, "Durations (seconds):\n"));
        for (key, &dur) in &self.durations {
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
        let mut r = Report::new();
        assert_eq!(r.get_count("block.read.count"), 0);
        r.increment("block.read.count", 1);
        assert_eq!(r.get_count("block.read.count"), 1);
        r.increment("block.read.count", 10);
        assert_eq!(r.get_count("block.read.count"), 11);
    }

    #[test]
    pub fn merge_reports() {
        let mut r1 = Report::new();
        let mut r2 = Report::new();
        r1.increment("block.read.count", 1);
        r1.increment("block.read.corrupt", 2);
        r2.increment("block.write.count", 1);
        r2.increment("block.read.corrupt", 10);
        r2.increment_size("block.write", 300, 100);
        r2.increment_duration("test", Duration::new(5, 0));
        r1.merge_from(&r2);
        assert_eq!(r1.get_count("block.read.count"), 1);
        assert_eq!(r1.get_count("block.read.corrupt"), 12);
        assert_eq!(r1.get_count("block.write.count"), 1);
        assert_eq!(r1.get_size("block.write"), (300, 100));
        assert_eq!(r1.get_duration("test"), Duration::new(5, 0));
    }

    #[test]
    pub fn display() {
        let mut r1 = Report::new();
        r1.increment("block.read.count", 10);
        r1.increment("block.write.count", 5);
        r1.increment_size("block.write", 300, 100);
        r1.increment_duration("test", Duration::new(42, 479760000));

        let formatted = format!("{}", r1);
        assert_eq!(formatted, "\
Counts:
  block.read.count                                          10
  block.write.count                                          5
Bytes (before and after compression):
  block.write                                              300       100        67%
Durations (seconds):
  test                                                  42.479
");
    }
}
