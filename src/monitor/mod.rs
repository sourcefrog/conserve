// Copyright 2023 Martin Pool

//! Communication from the library to a monitor: a test, a UI, etc.

pub mod collect;
mod counters;

use std::fmt::Debug;

use strum_macros::{EnumCount, EnumIter};

use crate::Apath;

pub use counters::Counters;

pub trait Monitor {
    /// Notify that a counter increased by a given amount.
    fn count(&self, counter: Counter, increment: usize);

    /// Set the absolute value of a counter.
    fn set_counter(&self, counter: Counter, value: usize);

    /// Notify that a problem occurred.
    fn problem(&self, problem: Problem);

    /// Started processing a file. Multiple files may be processed concurrently.
    fn start_file(&self, apath: &Apath);

    /// Finished processing a file.
    fn stop_file(&self, apath: &Apath);
}

#[derive(Debug)]
pub enum Problem {
    /// Some generic error.
    Error(crate::Error),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, EnumCount, EnumIter)]
pub enum Counter {
    BandsDone,
    BandsTotal,
    FilesDone,
    IndexBytesDone,
    BlockBytesDone,
    BlockRead,
    BlockWrite,
    BlockMatchExisting,
    BlockCacheHit,
    // ...
}
