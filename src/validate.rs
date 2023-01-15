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
use std::time::Instant;

use crate::*;

#[derive(Debug, Default)]
pub struct ValidateOptions {
    /// Assume blocks that are present have the right content: don't read and hash them.
    pub skip_block_hashes: bool,
}

pub(crate) fn validate_bands(
    archive: &Archive,
    band_ids: &[BandId],
) -> (HashMap<BlockHash, u64>, ValidateStats) {
    let mut stats = ValidateStats::default();
    let mut block_lens = HashMap::new();
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
                for (bh, bl) in st_block_lens {
                    block_lens
                        .entry(bh)
                        .and_modify(|al| *al = max(*al, bl))
                        .or_insert(bl);
                }
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

pub(crate) fn validate_stored_tree(
    st: &StoredTree,
) -> Result<(HashMap<BlockHash, u64>, ValidateStats)> {
    let mut block_lens = HashMap::new();
    let stats = ValidateStats::default();
    for entry in st
        .iter_entries(Apath::root(), Exclude::nothing())?
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
    Ok((block_lens, stats))
}
