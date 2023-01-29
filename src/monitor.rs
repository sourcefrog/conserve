// Copyright 2017-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use std::fmt::Debug;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

/// A Monitor is an abstracted way to show progress during an operation.
pub trait Monitor: Send + Sync {
    /// Update that some progress has been made on a task.
    fn progress(&self, progress: Progress);

    /// Return a reference to a counter struct holding atomic performance
    /// counters.
    fn counters(&self) -> &Counters;
}

/// Overall progress state communicated from Conserve core to the monitor.
#[derive(Clone)]
pub enum Progress {
    None,
    ValidateBands {
        total_bands: usize,
        bands_done: usize,
        start: Instant,
    },
    ValidateBlocks {
        blocks_done: usize,
        total_blocks: usize,
        bytes_done: u64,
        start: Instant,
    },
}

/// A ValidateMonitor that collects all errors without drawing anything,
/// for use in tests.
///
/// Errors are collected as strings, because not all of them can be cloned.
#[derive(Default, Debug)]
pub struct CollectMonitor {
    counters: Counters,
}

impl CollectMonitor {
    pub fn new() -> Self {
        CollectMonitor::default()
    }
}

impl Monitor for CollectMonitor {
    fn progress(&self, _progress: Progress) {}

    fn counters(&self) -> &Counters {
        &self.counters
    }
}

/// Counters of interesting performance events during an operation.
///
/// All the members are atomic so they can be updated through a shared
/// reference at any time.
#[derive(Default, Debug)]
pub struct Counters {
    // CAUTION: Don't use AtomicU64 here because it won't exist on
    // 32-bit platforms.
    pub blocks_read: AtomicUsize,
}
