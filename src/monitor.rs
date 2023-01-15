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

use std::fmt::{self, Debug};
use std::sync::Mutex;
use std::time::Instant;

use crate::{Error, Result};

/// A Monitor collects progress and problem findings during some high-level
/// operation such as a backup or validation.
///
/// Events reported to the Monitor can be, for example, drawn into a UI,
/// written to logs, or written
/// out as structured data.
pub trait Monitor: Send + Sync {
    /// The monitor is informed that a non-fatal error occurred.
    fn problem(&self, problem: Error) -> Result<()>;

    /// The task entered a new high-level phase; there's only one phase
    /// at a time.
    fn start_phase(&mut self, phase: Phase);

    /// Update that some progress has been made on a task.
    fn progress(&self, progress: Progress);
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

/// A ValidateMonitor that collects all problems without drawing anything,
/// for use in tests.
#[derive(Debug)]
pub struct CollectMonitor {
    pub problems: Mutex<Vec<Error>>,
    // pub phases: Vec<ValidatePhase>,
}

impl CollectMonitor {
    pub fn new() -> Self {
        CollectMonitor {
            problems: Mutex::new(Vec::new()),
            // phases: Vec::new(),
        }
    }

    pub fn into_problems(self) -> Vec<Error> {
        self.problems.into_inner().unwrap()
    }
}

impl Monitor for CollectMonitor {
    fn problem(&self, problem: Error) -> Result<()> {
        self.problems.lock().unwrap().push(problem);
        Ok(())
    }

    fn start_phase(&mut self, _phase: Phase) {}

    fn progress(&self, _progress: Progress) {}
}

#[non_exhaustive]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Phase {
    CheckArchiveDirectory,
    ListBlocks,
    ListBands,
    CheckIndexes(usize),
    CheckBlockContent { n_blocks: usize },
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Phase::CheckArchiveDirectory => write!(f, "Check archive directory"),
            Phase::ListBlocks => write!(f, "List blocks"),
            Phase::ListBands => write!(f, "List bands"),
            Phase::CheckIndexes(n) => write!(f, "Check {n} indexes"),
            Phase::CheckBlockContent { n_blocks } => {
                write!(f, "Check content of {n_blocks} blocks")
            }
        }
    }
}
