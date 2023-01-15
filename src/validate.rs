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
use std::fmt::Debug;
use std::time::Instant;

#[allow(unused_imports)]
use tracing::{info, warn};

use crate::blockdir::Address;
use crate::monitor::{Monitor, Progress};
use crate::*;

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
pub(crate) fn validate_bands<MO: Monitor>(
    archive: &Archive,
    band_ids: &[BandId],
    monitor: &mut MO,
) -> Result<ReferencedBlockLengths> {
    let mut block_lens = ReferencedBlockLengths::new();
    let start = Instant::now();
    let total_bands = band_ids.len();
    let mut bands_done = 0;
    'band: for band_id in band_ids {
        bands_done += 1;
        if let Err(err) = Band::open(archive, band_id).and_then(|band| band.validate(monitor)) {
            monitor.problem(err)?;
            continue 'band;
        };
        if let Err(err) = archive
            .open_stored_tree(BandSelectionPolicy::Specified(band_id.clone()))
            .and_then(|st| validate_stored_tree(&st))
            .map(|st_block_lens| block_lens.update(st_block_lens))
        {
            monitor.problem(err)?;
            continue 'band;
        }
        monitor.progress(Progress::ValidateBands {
            total_bands,
            bands_done,
            start,
        });
    }
    monitor.progress(Progress::None);
    Ok(block_lens)
}

fn validate_stored_tree(st: &StoredTree) -> Result<ReferencedBlockLengths> {
    let mut block_lens = ReferencedBlockLengths::new();
    // TODO: Maybe check entry ordering and other invariants.
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
