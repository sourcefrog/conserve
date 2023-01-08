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
use std::fmt;
use std::fmt::Debug;
use std::io::{Sink, Write};
use std::time::Instant;

use serde::Serialize;
use tracing::{info, warn};

use crate::blockdir::Address;
use crate::*;

/// A ValidateMonitor collects progress and problem findings during validation.
///
/// These can be, for example, drawn into a UI, written to logs, or written
/// out as structured data.
pub trait ValidateMonitor {
    /// The monitor is informed that a problem was found in the archive.
    fn problem(&mut self, problem: Problem) -> Result<()>;

    /// The monitor is informed that a phase of validation has started.
    fn start_phase(&mut self, phase: ValidatePhase);
}

/// A ValidateMonitor that logs messages, collects problems in memory, optionally
/// writes problems to a json file, and draws console progress bars.
#[derive(Debug)]
pub struct GeneralValidateMonitor<JF>
where
    JF: Write + Debug,
{
    pub progress_bars: bool,
    /// Optionally write all problems as json to this file as they're discovered.
    pub problems_json: Option<Box<JF>>,
    pub log_problems: bool,
    pub log_phases: bool,
    /// Accumulates all problems seen.
    pub problems: Vec<Problem>,
}

impl<JF> GeneralValidateMonitor<JF>
where
    JF: Write + Debug,
{
    pub fn new(problems_json: Option<JF>) -> Self {
        GeneralValidateMonitor {
            progress_bars: true,
            problems_json: problems_json.map(|x| Box::new(x)),
            log_problems: true,
            log_phases: true,
            problems: Vec::new(),
        }
    }
}

impl GeneralValidateMonitor<Sink> {
    pub fn without_file() -> GeneralValidateMonitor<Sink> {
        GeneralValidateMonitor::new(None::<Sink>)
    }
}

impl<JF> ValidateMonitor for GeneralValidateMonitor<JF>
where
    JF: Write + Debug,
{
    fn problem(&mut self, problem: Problem) -> Result<()> {
        if self.log_problems {
            warn!("{problem:?}"); // TODO: impl Display for Problem
        }
        if let Some(f) = self.problems_json.as_mut() {
            serde_json::to_writer_pretty(f, &problem)
                .map_err(|source| Error::SerializeProblem { source })?;
        }
        self.problems.push(problem);
        Ok(())
    }

    fn start_phase(&mut self, phase: ValidatePhase) {
        if self.log_phases {
            info!("{phase}");
        }
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

/// A type of problem in the archive that can be found during validation.
///
/// A problem is distinct from an error that it does not immediately terminate
/// validation.
///
/// This enum squashes error messages to strings so that they're easily serialized.
#[non_exhaustive]
#[derive(Debug, Serialize)]
pub enum Problem {
    BandOpenFailed {
        band_id: BandId,
        error_message: String,
    },
    BlockMissing(BlockHash),
    BlockCorrupt(BlockHash),
    ShortBlock {
        block_hash: BlockHash,
        actual_len: u64,
        referenced_len: u64,
    },
    UnexpectedFile(String),
    DuplicateBandDirectory(BandId),
    IoError {
        error_message: String,
        url: String,
    },
}

impl Problem {
    pub fn io_error(error: &dyn std::error::Error, transport: &dyn Transport) -> Self {
        Problem::IoError {
            error_message: error.to_string(),
            url: transport.url(),
        }
    }
}

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

pub(crate) fn validate_bands(
    archive: &Archive,
    band_ids: &[BandId],
) -> (ReferencedBlockLengths, ValidateStats) {
    let mut stats = ValidateStats::default();
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
    for band_id in band_ids {
        if let Ok(b) = Band::open(archive, band_id) {
            if b.validate(&mut stats).is_err() {
                stats.band_metadata_problems += 1;
            }
        } else {
            stats.band_open_errors += 1;
            continue;
        }
        if let Ok(st) = archive.open_stored_tree(BandSelectionPolicy::Specified(band_id.clone())) {
            if let Ok((st_block_lens, st_stats)) = validate_stored_tree(&st) {
                stats += st_stats;
                block_lens.update(st_block_lens);
            } else {
                stats.tree_validate_errors += 1
            }
        } else {
            stats.tree_open_errors += 1;
            continue;
        }
        view.update(|model| model.bands_done += 1);
    }
    (block_lens, stats)
}

fn validate_stored_tree(st: &StoredTree) -> Result<(ReferencedBlockLengths, ValidateStats)> {
    let mut block_lens = ReferencedBlockLengths::new();
    let stats = ValidateStats::default();
    for entry in st
        .iter_entries(Apath::root(), Exclude::nothing())?
        .filter(|entry| entry.kind() == Kind::File)
    {
        for addr in entry.addrs {
            block_lens.add(addr)
        }
    }
    Ok((block_lens, stats))
}
