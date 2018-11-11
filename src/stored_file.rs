// Copyright 2017, 2018 Martin Pool.

///! Access a file stored in the archive.
// use rayon::prelude::*;
use crate::*;

/// Returns the contents of a file stored in the archive, as an iter of byte blocks.
///
/// These can be constructed through `StoredTree::open_stored_file()` or more
/// generically through `ReadTree::file_contents`.
#[derive(Debug)]
pub struct StoredFile {
    block_dir: BlockDir,

    /// All addresses for this file.
    addrs: Vec<blockdir::Address>,

    report: Report,
}

impl StoredFile {
    /// Open a stored file.
    pub fn open(block_dir: BlockDir, addrs: Vec<blockdir::Address>, report: &Report) -> StoredFile {
        StoredFile {
            block_dir,
            addrs,
            report: report.clone(),
        }
    }

    /// Return a iterator of chunks of the file, as they're stored.
    pub fn content_chunks(&self) -> impl Iterator<Item = Result<Vec<u8>>> + '_ {
        let block_dir = self.block_dir.clone();
        let report = self.report.clone();
        self.addrs.iter().map(move |a| block_dir.get(&a, &report))
    }

    /// Validate the stored file hash is as expected.
    pub(crate) fn validate(&self, _apath: &Apath) -> Result<()> {
        // TODO: Perhaps the file should know its apath and hold its entry.
        // TODO: Give a more specific message including the band and apath, if
        // the content can't be loaded.
        // TODO: Arguably we don't need to actually load the chunks here; it's
        // enough to remember that all the blocks were loaded before.
        for c in self.content_chunks() {
            let c = c?;
            self.report.increment_work(c.len() as u64);
        }
        Ok(())
    }

    /// Open a cursor on this file that implements `std::io::Read`.
    pub(crate) fn as_read(self) -> ReadStoredFile {
        ReadStoredFile {
            remaining_addrs: self.addrs.into_iter(),
            buf: Vec::<u8>::new(),
            buf_cursor: 0,
            block_dir: self.block_dir,
            report: self.report,
        }
    }
}

/// Adapt a StoredFile to `std::io::Read`, which requires keeping a cursor position.
#[derive(Debug)]
pub struct ReadStoredFile {
    /// Block addresses remaining to be read.
    remaining_addrs: std::vec::IntoIter<blockdir::Address>,

    // TODO: buf, buf_cursor, remaining_addrs all really belong in some kind of `Read` adapter, not
    // the StoredFile itself.
    /// Already-read but not yet returned data.
    buf: Vec<u8>,

    /// How far through buf has been returned?
    buf_cursor: usize,

    block_dir: BlockDir,
    report: Report,
}

impl std::io::Read for ReadStoredFile {
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
            } else if let Some(addr) = self.remaining_addrs.next() {
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
