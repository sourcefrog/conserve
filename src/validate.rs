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

use std::cmp::max;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::time::Instant;

#[allow(unused_imports)]
use tracing::{info, warn};

use crate::blockdir::Address;
use crate::Error;
use crate::*;

/// A ValidateMonitor collects progress and problem findings during validation.
///
/// These can be, for example, drawn into a UI, written to logs, or written
/// out as structured data.
pub trait ValidateMonitor {
    /// The monitor is informed that a non-fatal error occurred while validating the
    /// archive.
    fn problem(&mut self, problem: Error) -> Result<()>;

    /// The monitor is informed that a phase of validation has started.
    fn start_phase(&mut self, phase: ValidatePhase);
}

/// A ValidateMonitor that collects all events without drawing anything,
/// for use in tests.
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

/// Options to [Archive::validate].
#[derive(Debug, Default)]
pub struct ValidateOptions {
    /// Assume blocks that are present have the right content: don't read and hash them.
    pub skip_block_hashes: bool,
}

// TODO: maybe this doesn't need to be a struct, but just a map updated by
// some functions...
pub(crate) struct ReferencedBlockLengths(pub(crate) HashMap<BlockHash, u64>);

impl ReferencedBlockLengths {
    fn new() -> ReferencedBlockLengths {
        ReferencedBlockLengths(HashMap::new())
    }

    fn add(&mut self, addr: Address) {
        let end = addr.start + addr.len;
        if let Some(al) = self.0.get_mut(&addr.hash) {
            *al = max(*al, end)
        } else {
            self.0.insert(addr.hash, end);
        }
    }

    fn update(&mut self, b: ReferencedBlockLengths) {
        for (bh, bl) in b.0 {
            self.0
                .entry(bh)
                .and_modify(|al| *al = max(*al, bl))
                .or_insert(bl);
        }
    }
}

/// Validate the indexes of all bands.
///
/// Returns the lengths of all blocks that were referenced, so that the caller can check
/// that all blocks are present and long enough.
pub(crate) fn validate_bands(
    archive: &Archive,
    band_ids: &[BandId],
    monitor: &mut dyn ValidateMonitor,
) -> Result<ReferencedBlockLengths> {
    let mut block_lens = ReferencedBlockLengths::new();
    struct ProgressModel {
        bands_done: usize,
        bands_total: usize,
        start: Instant,
    }
    impl nutmeg::Model for ProgressModel {
        fn render(&mut self, _width: usize) -> String {
            format!(
                "Check index {}/{}, {} done, {} remaining",
                self.bands_done,
                self.bands_total,
                nutmeg::percent_done(self.bands_done, self.bands_total),
                nutmeg::estimate_remaining(&self.start, self.bands_done, self.bands_total)
            )
        }
    }
    let view = nutmeg::View::new(
        ProgressModel {
            start: Instant::now(),
            bands_done: 0,
            bands_total: band_ids.len(),
        },
        ui::nutmeg_options(),
    );
    'band: for band_id in band_ids {
        match Band::open(archive, band_id) {
            Ok(band) => band.validate(monitor)?,
            Err(err) => {
                monitor.problem(err)?;
                continue;
            }
        };
        match archive.open_stored_tree(BandSelectionPolicy::Specified(band_id.clone())) {
            Ok(st) => {
                if let Ok(st_block_lens) = validate_stored_tree(&st, monitor) {
                    block_lens.update(st_block_lens);
                }
            }
            Err(err) => {
                monitor.problem(err)?;
                continue 'band;
            }
        }
        view.update(|model| model.bands_done += 1);
    }
    Ok(block_lens)
}

fn validate_stored_tree(
    st: &StoredTree,
    _monitor: &mut dyn ValidateMonitor,
) -> Result<ReferencedBlockLengths> {
    let mut block_lens = ReferencedBlockLengths::new();
    for entry in st
        .iter_entries(Apath::root(), Exclude::nothing())?
        .filter(|entry| entry.kind() == Kind::File)
    {
        for addr in entry.addrs {
            block_lens.add(addr)
        }
    }
    Ok(block_lens)
}
