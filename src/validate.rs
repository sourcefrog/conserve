// Copyright 2017, 2018, 2019, 2020, 2021, 2022 Martin Pool.

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

use crate::blockdir::Address;
use crate::ui::LinearModel;
use crate::*;

pub(crate) struct BlockLengths(pub(crate) HashMap<BlockHash, u64>);

#[derive(Debug, Default)]
pub struct ValidateOptions {
    /// Assume blocks that are present have the right content: don't read and hash them.
    pub skip_block_hashes: bool,
}

impl BlockLengths {
    fn new() -> BlockLengths {
        BlockLengths(HashMap::new())
    }

    fn add(&mut self, addr: Address) {
        let end = addr.start + addr.len;
        if let Some(al) = self.0.get_mut(&addr.hash) {
            *al = max(*al, end)
        } else {
            self.0.insert(addr.hash, end);
        }
    }

    fn update(&mut self, b: BlockLengths) {
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
) -> (BlockLengths, ValidateStats) {
    let mut stats = ValidateStats::default();
    let mut block_lens = BlockLengths::new();
    let view = nutmeg::View::new(
        LinearModel::new("Check index", band_ids.len()),
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
        view.update(|model| model.i += 1);
    }
    (block_lens, stats)
}

pub(crate) fn validate_stored_tree(st: &StoredTree) -> Result<(BlockLengths, ValidateStats)> {
    let mut block_lens = BlockLengths::new();
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
