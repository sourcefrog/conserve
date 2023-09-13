// Copyright 2017, 2018, 2019 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Access a file stored in the archive.
use crate::stats::Sizes;
use crate::*;

/// Returns the contents of a file stored in the archive, as an iter of byte blocks.
///
/// These can be constructed through `StoredTree::open_stored_file()`.
pub struct StoredFile {
    block_dir: BlockDir,

    /// All addresses for this file.
    addrs: Vec<blockdir::Address>,
}

impl StoredFile {
    /// Open a stored file.
    pub fn open(block_dir: BlockDir, addrs: Vec<blockdir::Address>) -> StoredFile {
        StoredFile { block_dir, addrs }
    }
}

impl ReadBlocks for StoredFile {
    fn num_blocks(&self) -> Result<usize> {
        Ok(self.addrs.len())
    }

    /// Return the content of the ith address in this file.
    // TODO: Not the best name, because it doesn't return the whole block...
    fn read_block(&self, i: usize) -> Result<(Vec<u8>, Sizes)> {
        self.block_dir.get(&self.addrs[i])
    }
}
