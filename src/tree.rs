// Conserve backup system.
// Copyright 2017, 2018, 2019, 2020, 2022 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Abstract Tree trait.

use std::ops::Range;

use crate::stats::Sizes;
use crate::*;

/// Abstract Tree that may be either on the real filesystem or stored in an archive.
pub trait ReadTree {
    type Entry: Entry + 'static;
    type R: std::io::Read;
    type IT: Iterator<Item = Self::Entry>;

    /// Iterate, in apath order, all the entries in this tree.
    ///
    /// Errors reading individual paths or directories are sent to the UI and
    /// counted, but are not treated as fatal, and don't appear as Results in the
    /// iterator.
    fn iter_entries(&self, subtree: Apath, exclude: Exclude) -> Result<Self::IT>;

    /// Read file contents as a `std::io::Read`.
    // TODO: Remove this and use ReadBlocks or similar.
    fn file_contents(&self, entry: &Self::Entry) -> Result<Self::R>;

    /// Estimate the number of entries in the tree.
    /// This might do somewhat expensive IO, so isn't the Iter's `size_hint`.
    fn estimate_count(&self) -> Result<u64>;

    /// Measure the tree size.
    ///
    /// This typically requires walking all entries, which may take a while.
    fn size(&self, exclude: Exclude) -> Result<TreeSize> {
        struct Model {
            files: usize,
            total_bytes: u64,
        }
        impl nutmeg::Model for Model {
            fn render(&mut self, _width: usize) -> String {
                format!(
                    "Measuring... {} files, {} MB",
                    self.files,
                    self.total_bytes / 1_000_000
                )
            }
        }
        let progress = nutmeg::View::new(
            Model {
                files: 0,
                total_bytes: 0,
            },
            ui::nutmeg_options(),
        );
        let mut tot = 0u64;
        for e in self.iter_entries(Apath::root(), exclude)? {
            // While just measuring size, ignore directories/files we can't stat.
            if let Some(bytes) = e.size() {
                tot += bytes;
                progress.update(|model| {
                    model.files += 1;
                    model.total_bytes += bytes;
                });
            }
        }
        Ok(TreeSize { file_bytes: tot })
    }
}

/// Read a file as a series of blocks of bytes.
///
/// When reading from the archive, the blocks are whatever size is stored.
/// When reading from the filesystem they're MAX_BLOCK_SIZE. But the caller
/// shouldn't assume the size.
pub trait ReadBlocks {
    /// Return a range of integers indexing the blocks (starting from 0.)
    fn num_blocks(&self) -> Result<usize>;

    fn block_range(&self) -> Result<Range<usize>> {
        Ok(0..self.num_blocks()?)
    }

    /// Read one block and return it as a byte vec. Also returns the compressed and uncompressed
    /// sizes.
    fn read_block(&self, i: usize) -> Result<(Vec<u8>, Sizes)>;
}

/// The measured size of a tree.
pub struct TreeSize {
    pub file_bytes: u64,
}
