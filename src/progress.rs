// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Generic progress bar indications.

// static PROGRESS_IMPL;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::Instant;

static IMPL: RwLock<ProgressImpl> = RwLock::new(ProgressImpl::Null);

static NEXT_TASK_ID: AtomicUsize = AtomicUsize::new(0);

pub(crate) mod term;

/// How to show progress bars?
#[derive(Debug, Clone, Copy)]
pub enum ProgressImpl {
    Null,
    Terminal,
}

impl ProgressImpl {
    /// Make this the selected way to show progress bars.
    pub fn activate(self) {
        *IMPL.write().expect("locked progress impl") = self
    }

    fn remove_bar(&mut self, task: &mut Bar) {
        match self {
            ProgressImpl::Null => (),
            ProgressImpl::Terminal => term::remove_bar(task.bar_id),
        }
    }

    fn add_bar(&mut self) -> Bar {
        let bar_id = assign_new_bar_id();
        match self {
            ProgressImpl::Null => (),
            ProgressImpl::Terminal => term::add_bar(bar_id),
        }
        Bar { bar_id }
    }

    fn post(&self, task: &Bar, progress: Progress) {
        match self {
            ProgressImpl::Null => (),
            ProgressImpl::Terminal => term::update_bar(task.bar_id, progress),
        }
    }
}

/// Enable drawing progress bars, only if stdout is a tty.
///
/// Progress bars are off by default.
pub fn enable_progress(enabled: bool) {
    if enabled {
        ProgressImpl::Terminal.activate();
    } else {
        ProgressImpl::Null.activate();
    }
}

fn assign_new_bar_id() -> usize {
    NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed)
}

/// State of progress on one bar.
#[derive(Clone)]
pub enum Progress {
    None,
    Backup {
        filename: String,
        scanned_file_bytes: u64,
        scanned_dirs: usize,
        scanned_files: usize,
        entries_new: usize,
        entries_changed: usize,
        entries_unchanged: usize,
    },
    ListBlocks {
        count: usize,
    },
    MeasureTree {
        files: usize,
        total_bytes: u64,
    },
    ReferencedBlocks {
        bands_started: usize,
        total_bands: usize,
        references_found: usize,
    },
    Restore {
        filename: String,
        bytes_done: u64,
    },
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

/// A transient progress task. The UI may draw these as some kind of
/// progress bar.
#[derive(Debug)]
pub struct Bar {
    /// An opaque unique ID for each concurrent task.
    bar_id: usize,
}

impl Bar {
    #[must_use]
    pub fn new() -> Self {
        IMPL.write().expect("lock progress impl").add_bar()
    }

    pub fn post(&self, progress: Progress) {
        IMPL.read().unwrap().post(self, progress)
    }
}

impl Default for Bar {
    fn default() -> Self {
        Bar::new()
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        IMPL.write().expect("lock progress impl").remove_bar(self)
    }
}
