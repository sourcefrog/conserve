// Copyright 2017 Martin Pool.

///! Access a file stored in the archive.

use super::*;

/// Returns the contents of a file stored in the archive, as an iter of byte blocks.
///
/// These can be constructed through StoredTree::file_contents().
#[derive(Debug)]
pub struct StoredFile {
    block_dir: BlockDir,

    /// Block addresses remaining to be read.
    addrs: std::vec::IntoIter<block::Address>,

    report: Report,
}

impl StoredFile {
    /// Open a stored file.
    pub fn open(block_dir: BlockDir, addrs: Vec<block::Address>, report: &Report) -> StoredFile {
        StoredFile {
            block_dir: block_dir,
            addrs: addrs.into_iter(),
            report: report.clone(),
        }
    }
}

impl Iterator for StoredFile {
    type Item = Result<Vec<u8>>;

    /// Yield a series of uncompressed vecs of byte contents.
    fn next(&mut self) -> Option<Result<Vec<u8>>> {
        if let Some(addr) = self.addrs.next() {
            Some(self.block_dir.get(&addr, &self.report))
        } else {
            None
        }
    }
}
