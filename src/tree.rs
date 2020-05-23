// Conserve backup system.
// Copyright 2017, 2018, 2019, 2020 Martin Pool.

//! Abstract Tree trait.

use std::ops::Range;

use crate::*;

/// Abstract Tree that may be either on the real filesystem or stored in an archive.
pub trait ReadTree: HasReport {
    type Entry: Entry;
    type I: Iterator<Item = Self::Entry>;
    type R: std::io::Read;

    /// Iterate, in apath order, all the entries in this tree.
    ///
    /// Errors reading individual paths or directories are sent to the report
    /// but are not treated as fatal, and don't appear as Results in the
    /// iterator.
    fn iter_entries(&self, report: &Report) -> Result<Self::I>;

    /// Read file contents as a `std::io::Read`.
    fn file_contents(&self, entry: &Self::Entry) -> Result<Self::R>;

    /// Estimate the number of entries in the tree.
    /// This might do somewhat expensive IO, so isn't the Iter's `size_hint`.
    fn estimate_count(&self) -> Result<u64>;

    /// Measure the tree size.
    ///
    /// This typically requires walking all entries, which may take a while.
    fn size(&self) -> Result<TreeSize> {
        let mut tot = 0u64;
        for e in self.iter_entries(self.report())? {
            // While just measuring size, ignore directories/files we can't stat.
            let s = e.size().unwrap_or(0);
            tot += s;
            ui::increment_bytes_done(s);
        }
        Ok(TreeSize { file_bytes: tot })
    }
}

/// A tree open for writing, either local or an an archive.
///
/// This isn't a sub-trait of ReadTree since a backup band can't be read while writing is
/// still underway.
///
/// Entries must be written in Apath order, since that's a requirement of the index.
pub trait WriteTree {
    fn finish(&mut self) -> Result<()>;

    /// Copy a directory entry from a source tree to this tree.
    fn copy_dir<E: Entry>(&mut self, entry: &E) -> Result<()>;

    /// Copy a symlink entry from a source tree to this tree.
    fn copy_symlink<E: Entry>(&mut self, entry: &E) -> Result<()>;

    /// Copy in the contents of a file from another tree.
    ///
    /// Returns Sizes describing the compressed and uncompressed sizes copied.
    // TODO: Use some better interface than IO::Read, that permits getting sizes
    // from the source file when restoring.
    fn copy_file<R: ReadTree>(&mut self, entry: &R::Entry, from_tree: &R) -> Result<Sizes>;
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
