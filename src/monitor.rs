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
use std::sync::Mutex;
use std::time::Instant;

use crate::Error;

/// A Monitor collects progress and error findings during some high-level
/// operation such as a backup or validation.
///
/// Events reported to the Monitor can be, for example, drawn into a UI,
/// written to logs, or written
/// out as structured data.
pub trait Monitor: Send + Sync {
    /// The monitor is informed that a non-fatal error occurred.
    fn error(&self, err: &Error);

    /// Record a less severe error.
    fn warning(&self, err: &Error);

    /// Return true if any errors were observed.
    fn had_errors(&self) -> bool;

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
    pub error_messages: Mutex<Vec<String>>,
    counters: Counters,
}

impl CollectMonitor {
    pub fn new() -> Self {
        CollectMonitor::default()
    }

    pub fn error_messages(self) -> Vec<String> {
        self.error_messages.into_inner().unwrap()
    }
}

impl Monitor for CollectMonitor {
    fn error(&self, err: &Error) {
        self.error_messages.lock().unwrap().push(err.to_string());
    }

    fn warning(&self, err: &Error) {
        self.error(err)
    }

    fn progress(&self, _progress: Progress) {}

    fn counters(&self) -> &Counters {
        &self.counters
    }

    fn had_errors(&self) -> bool {
        !self.error_messages.lock().unwrap().is_empty()
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
