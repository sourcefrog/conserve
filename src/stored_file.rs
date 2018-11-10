// Copyright 2017, 2018 Martin Pool.

///! Access a file stored in the archive.
// use rayon::prelude::*;
use blake2_rfc::blake2b::Blake2b;

use rustc_serialize::hex::ToHex;

use crate::blockdir::BLAKE_HASH_SIZE_BYTES;
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

    /// Block addresses remaining to be read.
    remaining_addrs: std::vec::IntoIter<blockdir::Address>,

    // TODO: buf, buf_cursor, remaining_addrs all really belong in some kind of `Read` adapter, not
    // the StoredFile itself.
    /// Already-read but not yet returned data.
    buf: Vec<u8>,

    /// How far through buf has been returned?
    buf_cursor: usize,

    report: Report,
}

impl StoredFile {
    /// Open a stored file.
    pub fn open(block_dir: BlockDir, addrs: Vec<blockdir::Address>, report: &Report) -> StoredFile {
        let remaining_addrs = addrs.clone().into_iter();
        StoredFile {
            block_dir,
            addrs,
            remaining_addrs,
            report: report.clone(),
            buf: Vec::<u8>::new(),
            buf_cursor: 0,
        }
    }

    /// Return a iterator of chunks of the file, as they're stored.
    pub fn content_chunks(&self) -> impl Iterator<Item = Result<Vec<u8>>> + '_ {
        let block_dir = self.block_dir.clone();
        let report = self.report.clone();
        self.addrs.iter().map(move |a| block_dir.get(&a, &report))
    }

    /// Validate the stored file hash is as expected.
    pub(crate) fn validate(&self, apath: &Apath, expected_hex: &str) -> Result<()> {
        // TODO: Perhaps the file should know its apath and hold its entry.
        let mut file_hasher = Blake2b::new(BLAKE_HASH_SIZE_BYTES);
        for c in self.content_chunks() {
            let c = c?;
            file_hasher.update(&c);
            self.report.increment_work(c.len() as u64);
        }
        let actual_hex = file_hasher.finalize().as_bytes().to_hex();
        if actual_hex != *expected_hex {
            Err(Error::FileCorrupt {
                apath: apath.clone(),
                expected_hex: expected_hex.to_string(),
                actual_hex,
            })
        } else {
            Ok(())
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
