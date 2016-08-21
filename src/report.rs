// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Count interesting events that occur during a run.

use std::collections;

#[allow(unused)]
static KNOWN_COUNTERS: &'static [&'static str] = &[
    "backup.file.count",
    "block.read.count",
    "block.read.corrupt",
    "block.read.misplaced",
    "block.write.already_present",
    "block.write.compressed_bytes",
    "block.write.count",
    "block.write.uncompressed_bytes",
    "index.write.compressed_bytes",
    "index.write.hunks",
    "source.selected.count",
    "source.skipped.unsupported_file_kind",
    "source.visited.directories.count",
];

/// A Report is notified of problems or non-problematic events that occur while Conserve is
/// running.
///
/// A Report holds counters, identified by a name.  All implicitly start at 0.  All the
/// counter
/// names must be static strings.
#[derive(Clone, Debug)]
pub struct Report {
    count: collections::HashMap<&'static str, u64>,
}

impl Report {
    pub fn new() -> Report {
        Report { count: collections::HashMap::new() }
    }

    /// Increment a counter by a given amount.
    ///
    /// The name must be a static string.  Counters implicitly start at 0.
    pub fn increment(self: &mut Report, counter_name: &'static str, delta: u64) {
        *self.count.entry(counter_name).or_insert(0) += delta;
    }

    /// Return the value of a counter.  A counter that has not yet been updated is 0.
    #[allow(unused)]
    pub fn get_count(self: &Report, counter_name: &str) -> u64 {
        *self.count.get(counter_name).unwrap_or(&0)
    }

    /// Merge the contents of `from_report` into `self`.
    pub fn merge_from(self: &mut Report, from_report: &Report) {
        for (name, value) in &from_report.count {
            self.increment(name, *value);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::Report;

    #[test]
    pub fn count() {
        let mut r = Report::new();
        assert_eq!(r.get_count("splines_reticulated"), 0);
        r.increment("splines_reticulated", 1);
        assert_eq!(r.get_count("splines_reticulated"), 1);
        r.increment("splines_reticulated", 10);
        assert_eq!(r.get_count("splines_reticulated"), 11);
    }

    #[test]
    pub fn merge() {
        let mut r1 = Report::new();
        let mut r2 = Report::new();
        r1.increment("a", 1);
        r1.increment("common", 2);
        r2.increment("inr2", 1);
        r2.increment("common", 10);
        r1.merge_from(&r2);
        assert_eq!(r1.get_count("a"), 1);
        assert_eq!(r1.get_count("common"), 12);
        assert_eq!(r1.get_count("inr2"), 1);
    }
}
