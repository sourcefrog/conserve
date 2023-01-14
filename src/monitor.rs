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

use crate::{Error, Result};

/// A ValidateMonitor collects progress and problem findings during validation.
///
/// These can be, for example, drawn into a UI, written to logs, or written
/// out as structured data.
pub trait ValidateMonitor: Debug + Send {
    /// The monitor is informed that a non-fatal error occurred while validating the
    /// archive.
    fn problem(&mut self, problem: Error) -> Result<()>;

    /// The monitor is informed that a phase of validation has started.
    fn start_phase(&mut self, phase: ValidatePhase);
}

/// A ValidateMonitor that collects all events without drawing anything,
/// for use in tests.
#[derive(Debug)]
pub struct CollectValidateMonitor {
    pub problems: Vec<Error>,
    pub phases: Vec<ValidatePhase>,
}

impl CollectValidateMonitor {
    pub fn new() -> Self {
        CollectValidateMonitor {
            problems: Vec::new(),
            phases: Vec::new(),
        }
    }
}

impl ValidateMonitor for CollectValidateMonitor {
    fn problem(&mut self, problem: Error) -> Result<()> {
        self.problems.push(problem);
        Ok(())
    }

    fn start_phase(&mut self, phase: ValidatePhase) {
        self.phases.push(phase)
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ValidatePhase {
    CheckArchiveDirectory,
    ListBlocks,
    ListBands,
    CheckIndexes(usize),
    CheckBlockContent { n_blocks: usize },
}

impl fmt::Display for ValidatePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidatePhase::CheckArchiveDirectory => write!(f, "Check archive directory"),
            ValidatePhase::ListBlocks => write!(f, "List blocks"),
            ValidatePhase::ListBands => write!(f, "List bands"),
            ValidatePhase::CheckIndexes(n) => write!(f, "Check {n} indexes"),
            ValidatePhase::CheckBlockContent { n_blocks } => {
                write!(f, "Check content of {n_blocks} blocks")
            }
        }
    }
}
