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
use std::sync::Arc;

use tracing::debug;

use crate::monitor::Monitor;
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
pub(crate) fn validate_bands(
    archive: &Archive,
    band_ids: &[BandId],
    monitor: Arc<dyn Monitor>,
) -> Result<HashMap<BlockHash, u64>> {
    let mut block_lens = HashMap::new();
    let task = monitor.start_task("Validate indexes".to_string());
    task.set_total(band_ids.len());
    'band: for band_id in band_ids.iter() {
        task.increment(1);
        let band = match Band::open(archive, *band_id) {
            Ok(band) => band,
            Err(err) => {
                monitor.error(err);
                continue 'band;
            }
        };
        if let Err(err) = band.validate() {
            monitor.error(err);
            continue 'band;
        };
        let st = match archive.open_stored_tree(BandSelectionPolicy::Specified(*band_id)) {
            Err(err) => {
                monitor.error(err);
                continue 'band;
            }
            Ok(st) => st,
        };
        let band_block_lens = match validate_stored_tree(&st, monitor.as_ref()) {
            Err(err) => {
                monitor.error(err);
                continue 'band;
            }
            Ok(block_lens) => block_lens,
        };
        merge_block_lens(&mut block_lens, &band_block_lens);
    }
    Ok(block_lens)
}

fn merge_block_lens(into: &mut HashMap<BlockHash, u64>, from: &HashMap<BlockHash, u64>) {
    for (bh, bl) in from {
        into.entry(bh.clone())
            .and_modify(|l| *l = max(*l, *bl))
            .or_insert(*bl);
    }
}

fn validate_stored_tree(st: &StoredTree, monitor: &dyn Monitor) -> Result<HashMap<BlockHash, u64>> {
    // TODO: Check other entry properties are correct.
    // TODO: Check they're in apath order.
    // TODO: Count progress for index blocks within one tree?
    let _task = monitor.start_task(format!("Validate stored tree {}", st.band().id()));
    let mut block_lens = HashMap::new();
    for entry in st
        .iter_entries(Apath::root(), Exclude::nothing())?
        .filter(|entry| entry.kind() == Kind::File)
    {
        // TODO: Read index hunks, count into the task per hunk. Then, we can
        // read hunks in parallel.
        for addr in entry.addrs {
            let end = addr.start + addr.len;
            block_lens
                .entry(addr.hash.clone())
                .and_modify(|l| *l = max(*l, end))
                .or_insert(end);
        }
    }
    debug!(blocks = %block_lens.len(), band_id = ?st.band().id(), "Validated stored tree");
    Ok(block_lens)
}
