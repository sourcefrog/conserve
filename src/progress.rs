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

use std::sync::RwLock;
use std::time::Instant;

static IMPL: RwLock<ProgressImpl> = RwLock::new(ProgressImpl::Null);

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
}

/// Overall progress state communicated from Conserve core to whatever progress bar
/// impl is in use.
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

impl Progress {
    /// Update the UI to show this progress state.
    pub fn post(self) {
        match *IMPL.read().unwrap() {
            ProgressImpl::Null => (),
            ProgressImpl::Terminal => crate::ui::termui::post_progress(self),
        }
    }
}
