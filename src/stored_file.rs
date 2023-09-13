// Copyright 2017, 2018, 2019, 2023 Martin Pool.

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
use crate::blockdir::Address;
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

    pub fn addresses(&self) -> &[Address] {
        self.addrs.as_ref()
    }

    /// Return an iterator of content parts, which when concatenated
    /// reconstruct the content of the file.
    pub fn content(&self) -> impl Iterator<Item = Result<Vec<u8>>> + '_ {
        self.addrs
            .iter()
            .map(|addr| self.block_dir.get(addr).map(|(bytes, _sizes)| bytes))
    }
}
