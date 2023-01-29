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
use tracing::{error, info, warn};

use crate::misc::ResultExt;
use crate::monitor::{Monitor, Progress};
use crate::*;

/// Options to [Archive::validate].
#[derive(Debug, Default)]
pub struct ValidateOptions {
    /// Assume blocks that are present have the right content: don't read and hash them.
    pub skip_block_hashes: bool,
}

/// Validate the indexes of all bands.
///
/// Returns the lengths of all blocks that were referenced, so that the caller can check
/// that all blocks are present and long enough.
pub(crate) fn validate_bands<MO: Monitor>(
    archive: &Archive,
    band_ids: &[BandId],
    monitor: &mut MO,
) -> Result<HashMap<BlockHash, u64>> {
    let mut block_lens = HashMap::new();
    let start = Instant::now();
    let total_bands = band_ids.len();
    'band: for (bands_done, band_id) in band_ids.iter().enumerate() {
        let band = match Band::open(archive, band_id) {
            Ok(band) => band,
            Err(err) => {
                error!(%err, %band_id, "Error opening band");
                continue 'band;
            }
        };
        if let Err(err) = band.validate(monitor) {
            error!(%err, %band_id, "Error validating band");
            continue 'band;
        };
        if let Err(err) = archive
            .open_stored_tree(BandSelectionPolicy::Specified(band_id.clone()))
            .and_then(|st| validate_stored_tree(&st, monitor))
            .map(|st_block_lens| merge_block_lens(&mut block_lens, &st_block_lens))
        {
            error!(%err, %band_id, "Error validating stored tree");
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

fn merge_block_lens(into: &mut HashMap<BlockHash, u64>, from: &HashMap<BlockHash, u64>) {
    for (bh, bl) in from {
        into.entry(bh.clone())
            .and_modify(|l| *l = max(*l, *bl))
            .or_insert(*bl);
    }
}

fn validate_stored_tree<MO: Monitor>(
    st: &StoredTree,
    _monitor: &mut MO,
) -> Result<HashMap<BlockHash, u64>> {
    let mut block_lens = HashMap::new();
    // TODO: Check other entry properties are correct.
    // TODO: Check they're in apath order.
    // TODO: Count progress for index blocks within one tree?
    for entry in st
        .iter_entries(Apath::root(), Exclude::nothing())
        .our_inspect_err(|err| error!(%err, "Error iterating index entries"))?
        .filter(|entry| entry.kind() == Kind::File)
    {
        for addr in entry.addrs {
            let end = addr.start + addr.len;
            block_lens
                .entry(addr.hash.clone())
                .and_modify(|l| *l = max(*l, end))
                .or_insert(end);
        }
    }
    Ok(block_lens)
}
