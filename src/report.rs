// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Count interesting events that occur during a run.

use std::collections;

/// A Report is notified of problems or non-problematic events that occur while Conserve is
/// running.
///
/// A Report holds counters, identified by a name.  All implicitly start at 0.  All the
/// counter
/// names must be static strings.
#[derive(Debug)]
pub struct Report {
    count: collections::HashMap<&'static str, u64>,
}

impl Report {
    pub fn new() -> Report {
        Report {
            count: collections::HashMap::new()
        }
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
}


#[cfg(test)]
mod tests {
    use super::Report;

    #[test]
    pub fn test_count() {
        let mut r = Report::new();
        assert_eq!(r.get_count("splines_reticulated"), 0);
        r.increment("splines_reticulated", 1);
        assert_eq!(r.get_count("splines_reticulated"), 1);
        r.increment("splines_reticulated", 10);
        assert_eq!(r.get_count("splines_reticulated"), 11);
    }
}
