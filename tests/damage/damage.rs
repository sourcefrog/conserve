// Conserve backup system.
// Copyright 2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Strategies for damaging files.

use std::fs::{remove_file, OpenOptions};
use std::path::{Path, PathBuf};

use conserve::monitor::test::TestMonitor;
use conserve::transport::open_local_transport;
use conserve::{Archive, BandId, BlockHash};
use itertools::Itertools;
use rayon::prelude::ParallelIterator;

/// A way of damaging a file in an archive.
#[derive(Debug, Clone)]
pub enum DamageAction {
    /// Truncate the file to zero bytes.
    Truncate,

    /// Delete the file.
    Delete,
    // TODO: Also test other types of damage, including
    // permission denied (as a kind of IOError), and binary junk.
}

impl DamageAction {
    /// Apply this damage to a file.
    ///
    /// The file must already exist.
    pub fn damage(&self, path: &Path) {
        assert!(path.exists(), "Path to be damaged does not exist: {path:?}");
        match self {
            DamageAction::Truncate => {
                OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(path)
                    .expect("truncate file");
            }
            DamageAction::Delete => {
                remove_file(path).expect("delete file");
            }
        }
    }
}

/// An abstract description of which file will be damaged.
///
/// Bands are identified by untyped integers for brevity in rstest names.
#[derive(Debug, Clone)]
pub enum DamageLocation {
    /// Delete the head of a band.
    BandHead(u32),
    BandTail(u32),
    /// Damage a block, identified by its index in the sorted list of all blocks in the archive,
    /// to avoid needing to hardcode a hash in the test.
    Block(usize),
    // TODO: Also test damage to other files: index hunks, archive header, etc.
}

impl DamageLocation {
    /// Find the specific path for this location, within an archive.
    pub fn to_path(&self, archive_dir: &Path) -> PathBuf {
        match self {
            DamageLocation::BandHead(band_id) => archive_dir
                .join(BandId::from(*band_id).to_string())
                .join("BANDHEAD"),
            DamageLocation::BandTail(band_id) => archive_dir
                .join(BandId::from(*band_id).to_string())
                .join("BANDTAIL"),
            DamageLocation::Block(block_index) => {
                let archive =
                    Archive::open(open_local_transport(archive_dir).expect("open transport"))
                        .expect("open archive");
                let block_dir = archive.block_dir();
                let block_hash = block_dir
                    .blocks(TestMonitor::arc())
                    .expect("list blocks")
                    .collect::<Vec<BlockHash>>()
                    .into_iter()
                    .sorted()
                    .nth(*block_index)
                    .expect("Archive has an nth block");
                archive_dir
                    .join("d")
                    .join(conserve::blockdir::block_relpath(&block_hash))
            }
        }
    }
}
