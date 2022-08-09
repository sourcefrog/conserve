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
use crate::*;

pub struct BlockLengths(pub HashMap<BlockHash, u64>);

#[derive(Debug, Default)]
pub struct ValidateOptions {
    /// Assume blocks that are present have the right content: don't read and hash them.
    pub skip_block_hashes: bool,
}

/// Band validation result.
pub enum BandValidateResult {
    MetadataError(Error),
    
    OpenError(Error),
    TreeOpenError(Error),
    
    TreeValidateError(Error),

    Valid(BlockLengths, ValidateStats),
}
pub enum BlockMissingReason {
    /// The target bock can not be found.
    NotExisting,

    /// The block reference points to an invalid data segment.
    InvalidRange,
}

pub enum BandProblem {
    MissingHeadFile{ band_head_filename: String },
    UnexpectedFiles{ files: Vec<String> },
    UnexpectedDirectories{ directories: Vec<String> }
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
    monitor: &dyn ValidateMonitor,
) -> (BlockLengths, ValidateStats) {
    let mut stats = ValidateStats::default();
    let mut block_lens = BlockLengths::new();
    
    for band_id in band_ids {
        monitor.validate_band(band_id);
        let result = validate_band(archive, &mut stats, band_id, monitor);
        monitor.validate_band_result(band_id, &result);
        
        match result {
            BandValidateResult::MetadataError(_) => stats.band_metadata_problems += 1,
            BandValidateResult::OpenError(_) => stats.band_open_errors += 1,
            BandValidateResult::TreeOpenError(_) => stats.tree_open_errors += 1,
            BandValidateResult::TreeValidateError(_) => stats.tree_validate_errors += 1,
            BandValidateResult::Valid(st_block_lens, st_stats) => {
                stats += st_stats;
                block_lens.update(st_block_lens);
            }
        }
    }

    (block_lens, stats)
}

pub(crate) fn validate_band(archive: &Archive, stats: &mut ValidateStats, band_id: &BandId, monitor: &dyn ValidateMonitor) -> BandValidateResult {
    let band = match Band::open(archive, band_id) {
        Ok(band) => band,
        Err(error) => return BandValidateResult::OpenError(error)
    };

    if let Err(error) = band.validate(stats, monitor) {
        return BandValidateResult::MetadataError(error);
    }

    let stored_tree = match archive.open_stored_tree(BandSelectionPolicy::Specified(band_id.clone())) {
        Ok(tree) => tree,
        Err(error) => return BandValidateResult::TreeOpenError(error),
    };

    match validate_stored_tree(&stored_tree)  {
        Ok(result) => BandValidateResult::Valid(result.0, result.1),
        Err(error) => BandValidateResult::TreeValidateError(error),
    }
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
