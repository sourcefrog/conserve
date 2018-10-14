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
    addrs: std::vec::IntoIter<blockdir::Address>,

    /// Already-read but not yet returned data.
    buf: Vec<u8>,

    /// How far through buf has been returned?
    buf_cursor: usize,

    report: Report,
}

impl StoredFile {
    /// Open a stored file.
    pub fn open(block_dir: BlockDir, addrs: Vec<blockdir::Address>, report: &Report) -> StoredFile {
        StoredFile {
            block_dir,
            addrs: addrs.into_iter(),
            report: report.clone(),
            buf: Vec::<u8>::new(),
            buf_cursor: 0,
        }
    }
}

impl std::io::Read for StoredFile {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        // TODO: Readahead n_cpus blocks into memory, using futures-cpupool or similar.
        loop {
            // If there's already buffered data, return as much of that as will fit.
            let avail = self.buf.len() - self.buf_cursor;
            if avail > 0 {
                let s = std::cmp::min(out.len(), avail);
                let r = &self.buf[self.buf_cursor..self.buf_cursor + s];
                out[..s].copy_from_slice(r);
                self.buf_cursor += s;
                return Ok(s);
            } else if let Some(addr) = self.addrs.next() {
                // TODO: Handle errors nicely, but they need to convert to std::io::Error.
                self.buf = self.block_dir.get(&addr, &self.report).unwrap();
                self.buf_cursor = 0;
            // TODO: Read directly into the caller's buffer, if it will fit. Requires changing
            // BlockDir::get to take a caller-provided buffer.
            } else {
                // No data buffered and no more to read, end of file.
                return Ok(0);
            }
        }
    }
}
