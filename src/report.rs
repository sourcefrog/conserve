// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! Accumulate statistics about a Conserve operation.
//!
//! A report includes counters of events, and also sizes for files.
//!
//! Sizes can be reported in both compressed and uncompressed form.
//!
//! By convention in this library when a Report is explicitly provided, it's the last parameter.
//!
//! When possible a Report is inherited from an `Archive` into the objects created from it.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::AddAssign;
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard};
use std::time;
use std::time::Duration;

use log;

use super::*;
use super::ui;
use super::ui::UI;
use super::ui::plain::PlainUI;

#[cfg_attr(rustfmt, rustfmt_skip)]
static KNOWN_COUNTERS: &'static [&'static str] = &[
    "dir",
    "file",
    "file.empty",
    "file.medium",
    "file.large",
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


static KNOWN_SIZES: &'static [&'static str] = &["block", "index"];

#[cfg_attr(rustfmt, rustfmt_skip)]
static KNOWN_DURATIONS: &'static [&'static str] = &[
    "block.compress",
    "block.hash",
    "block.write",
    "file.hash",
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
#[derive(Debug)]
pub struct Counts {
    count: BTreeMap<&'static str, u64>,
    sizes: BTreeMap<&'static str, Sizes>,
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
/// Cloning a Report makes a shared reference to the same underlying counters.
#[derive(Clone, Debug)]
pub struct Report {
    counts: Arc<Mutex<Counts>>,
    ui: Arc<Mutex<Box<UI + Send>>>,
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
        if let Some(c) = self.mut_counts().count.get_mut(counter_name) {
            *c += delta;
        } else {
            panic!("unregistered counter {:?}", counter_name);
        }
        self.ui.lock().unwrap().show_progress(
            &*self.borrow_counts(),
        );
    }

    pub fn increment_size(&self, counter_name: &str, sizes: Sizes) {
        let mut counts = self.mut_counts();
        let e = counts.sizes.get_mut(counter_name).expect(
            "unregistered size counter",
        );
        *e += sizes;
    }

    pub fn increment_duration(&self, name: &str, duration: Duration) {
        *self.mut_counts().durations.get_mut(name).expect(
            "undefined duration counter",
        ) += duration;
    }

    /// Merge the contents of `from_report` into `self`.
    pub fn merge_from(&self, from_report: &Report) {
        let from_counts = from_report.mut_counts();
        for (name, value) in &from_counts.count {
            self.increment(name, *value);
        }
        for (name, s) in &from_counts.sizes {
            self.increment_size(name, s.clone());
        }
        for (name, duration) in &from_counts.durations {
            self.increment_duration(name, *duration);
        }
    }

    pub fn measure_duration<T, F>(&self, duration_name: &str, closure: F) -> T
    where
        F: FnOnce() -> T,
    {
        let start = time::Instant::now();
        let result = closure();
        self.increment_duration(duration_name, start.elapsed());
        result
    }

    pub fn become_logger(&self, log_level: log::LogLevelFilter) {
        log::set_logger(|max_log_level| {
            max_log_level.set(log_level);
            Box::new(self.clone())
        }).ok();
    }

    pub fn get_size(&self, counter_name: &str) -> Sizes {
        self.borrow_counts().get_size(counter_name)
    }

    pub fn get_count(&self, counter_name: &str) -> u64 {
        self.borrow_counts().get_count(counter_name)
    }

    pub fn get_duration(&self, name: &str) -> Duration {
        self.borrow_counts().get_duration(name)
    }

    /// Report that processing started for a given entry.
    pub fn start_entry(&self, entry: &Entry) {
        // TODO: Leave cursor pending at the end of the line until it's finished?
        info!("{}", entry.apath());
    }
}


impl Display for Report {
    fn fmt(&self, f: &mut Formatter) -> std::result::Result<(), fmt::Error> {
        write!(f, "Counts:\n")?;
        let counts = self.mut_counts();
        for (key, value) in &counts.count {
            if *value > 0 {
                write!(f, "  {:<40} {:>9}\n", *key, *value)?;
            }
        }
        write!(f, "Bytes (before and after compression):\n")?;
        for (key, s) in &counts.sizes {
            if s.uncompressed > 0 {
                let ratio = ui::compression_ratio(s);
                write!(
                    f,
                    "  {:<40} {:>9} {:>9} {:>9.1}x\n",
                    *key,
                    s.uncompressed,
                    s.compressed,
                    ratio
                )?;
            }
        }
        write!(f, "Durations (seconds):\n")?;
        for (key, &dur) in &counts.durations {
            let millis = dur.subsec_nanos() / 1000000;
            let secs = dur.as_secs();
            if millis > 0 || secs > 0 {
                write!(f, "  {:<40} {:>5}.{:>03}\n", key, secs, millis)?;
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
        }
        let mut inner_sizes = BTreeMap::new();
        for counter_name in KNOWN_SIZES {
            inner_sizes.insert(*counter_name, Sizes::default());
        }
        let mut inner_durations: BTreeMap<&'static str, Duration> = BTreeMap::new();
        for name in KNOWN_DURATIONS {
            inner_durations.insert(name, Duration::new(0, 0));
        }
        Counts {
            count: inner_count,
            sizes: inner_sizes,
            durations: inner_durations,
            start: time::Instant::now(),
        }
    }

    pub fn get_duration(&self, name: &str) -> Duration {
        *self.durations.get(name).unwrap_or_else(|| {
            panic!("unknown duration {:?}", name)
        })
    }

    /// Return the value of a counter.  A counter that has not yet been updated is 0.
    pub fn get_count(&self, counter_name: &str) -> u64 {
        *self.count.get(counter_name).unwrap_or_else(|| {
            panic!("unknown counter {:?}", counter_name)
        })
    }

    /// Get size of data processed.
    ///
    /// For any size-counter name, returns a pair of (compressed, uncompressed) sizes,
    /// in bytes.
    pub fn get_size(&self, counter_name: &str) -> Sizes {
        *self.sizes.get(counter_name).unwrap_or_else(|| {
            panic!("unknown counter {:?}", counter_name)
        })
    }

    pub fn elapsed_time(&self) -> Duration {
        self.start.elapsed()
    }
}


#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::{Report, Sizes};

    #[test]
    pub fn count() {
        let r = Report::new();
        assert_eq!(r.borrow_counts().get_count("block.read"), 0);
        r.increment("block.read", 1);
        assert_eq!(r.borrow_counts().get_count("block.read"), 1);
        r.increment("block.read", 10);
        assert_eq!(r.borrow_counts().get_count("block.read"), 11);
    }

    #[test]
    pub fn merge_reports() {
        let r1 = Report::new();
        let r2 = Report::new();
        r1.increment("block.write", 1);
        r1.increment("block.corrupt", 2);
        r2.increment("block.write", 1);
        r2.increment("block.corrupt", 10);
        r2.increment_size(
            "block",
            Sizes {
                uncompressed: 300,
                compressed: 100,
            },
        );
        r2.increment_duration("test", Duration::new(5, 0));
        r1.merge_from(&r2);
        let cs = r1.borrow_counts();
        assert_eq!(cs.get_count("block.write"), 2);
        assert_eq!(cs.get_count("block.corrupt"), 12);
        assert_eq!(
            cs.get_size("block"),
            Sizes {
                uncompressed: 300,
                compressed: 100,
            }
        );
        assert_eq!(cs.get_duration("test"), Duration::new(5, 0));
    }

    #[cfg_attr(rustfmt, rustfmt_skip)]
    #[test]
    pub fn display() {
        let r1 = Report::new();
        r1.increment("block.write", 10);
        r1.increment("block.write", 5);
        r1.increment_size("block",
            Sizes { uncompressed: 300, compressed: 100 });
        r1.increment_duration("test", Duration::new(42, 479760000));

        let formatted = format!("{}", r1);
        assert_eq!(formatted, "\
Counts:
  block.write                                     15
Bytes (before and after compression):
  block                                          300       100       3.0x
Durations (seconds):
  test                                        42.479
");
    }
}
