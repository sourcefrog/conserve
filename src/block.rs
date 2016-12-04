// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! File contents are stored in data blocks within an archive band.

use std::fs;
use std::io;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use brotli2::write::BrotliEncoder;
use rustc_serialize::hex::ToHex;

use tempfile;

use super::errors::*;
use super::io::{read_and_decompress};
use super::report::Report;

/// Use the maximum 64-byte hash.
const BLAKE_HASH_SIZE_BYTES: usize = 64;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

/// The unique identifier for a block: its hexadecimal `BLAKE2b` hash.
pub type BlockHash = String;


/// Points to some compressed data inside the block dir.
///
/// Identifiers are: which file contains it, at what (pre-compression) offset,
/// and what (pre-compression) length.
#[derive(Debug, PartialEq, RustcDecodable, RustcEncodable)]
pub struct Address {
    /// ID of the block storing this info (in future, salted.)
    pub hash: String,

    /// Position in this block where data begins.
    pub start: u64,

    /// Length of this block to be used.
    pub len: u64,
}


/// A readable, writable directory within a band holding data blocks.
pub struct BlockDir {
    pub path: PathBuf,

    /// Internal-use buffer for reading.
    buf: Vec<u8>,
}

fn block_name_to_subdirectory(block_hash: &str) -> &str {
    &block_hash[..SUBDIR_NAME_CHARS]
}

impl BlockDir {
    /// Create a BlockDir accessing `path`, which must exist as a directory.
    pub fn new(path: &Path) -> BlockDir {
        BlockDir {
            path: path.to_path_buf(),
            buf: vec![],
        }
    }

    /// Return the subdirectory in which we'd put a file called `hash_hex`.
    fn subdir_for(self: &BlockDir, hash_hex: &str) -> PathBuf {
        let mut buf = self.path.clone();
        buf.push(block_name_to_subdirectory(hash_hex));
        buf
    }

    /// Return the full path for a file called `hex_hash`.
    fn path_for_file(self: &BlockDir, hash_hex: &str) -> PathBuf {
        let mut buf = self.subdir_for(hash_hex);
        buf.push(hash_hex);
        buf
    }

    pub fn store(&mut self, from_file: &mut Read, report: &Report) -> Result<(Vec<Address>, BlockHash)> {
        let tempf = try!(tempfile::NamedTempFileOptions::new()
            .prefix("tmp").create_in(&self.path));
        let mut encoder = BrotliEncoder::new(tempf, super::BROTLI_COMPRESSION_LEVEL);
        let mut hasher = Blake2b::new(BLAKE_HASH_SIZE_BYTES);
        let mut uncompressed_length: u64 = 0;
        const BUF_SIZE: usize = 1 << 20;
        if self.buf.len() < BUF_SIZE {
            self.buf.resize(BUF_SIZE, 0u8);
        }
        loop {
            let buf_slice = self.buf.as_mut_slice();
            let read_size = try!(report.measure_duration("source.read", || from_file.read(buf_slice)));
            let input = &buf_slice[.. read_size];
            if read_size == 0 { break; } // eof
            uncompressed_length += read_size as u64;

            try!(report.measure_duration("block.compress", || encoder.write_all(input)));
            report.measure_duration("block.hash", || hasher.update(input));
        }

        let mut tempf = try!(encoder.finish());
        let hex_hash = hasher.finalize().as_bytes().to_hex();

        // TODO: Update this when the stored blocks can be different from body hash.
        let refs = vec![Address {
            hash: hex_hash.clone(),
            start: 0,
            len: uncompressed_length,
        }];
        if try!(self.contains(&hex_hash)) {
            report.increment("block.already_present", 1);
            return Ok((refs, hex_hash));
        }
        let compressed_length: u64 = try!(tempf.seek(SeekFrom::Current(0)));
        try!(super::io::ensure_dir_exists(&self.subdir_for(&hex_hash)));
        // Also use plain `persist` not `persist_noclobber` to avoid calling `link` on Unix.
        if let Err(e) = tempf.persist(&self.path_for_file(&hex_hash)) {
            if e.error.kind() == io::ErrorKind::AlreadyExists {
                // Suprising we saw this rather than detecting it above.
                warn!("Unexpected late detection of existing block {:?}", hex_hash);
                report.increment("block.already_present", 1);
                return Ok((refs, hex_hash));
            } else {
                return Err(e.error.into());
            }
        }
        report.increment("block", 1);
        report.increment_size("block", uncompressed_length, compressed_length);
        Ok((refs, hex_hash))
    }

    /// True if the named block is present in this directory.
    pub fn contains(self: &BlockDir, hash: &str) -> Result<bool> {
        match fs::metadata(self.path_for_file(hash)) {
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Ok(_) => Ok(true),
            Err(e) => Err(e.into()),
        }
    }

    /// Read back the contents of a block, as a byte array.
    ///
    /// TODO: Return a Read rather than a Vec.
    pub fn get(self: &BlockDir, addr: &Address, report: &Report) -> Result<Vec<u8>> {
        // TODO: Accept vectors of multiple addresess, maybe in another function.
        let hash = &addr.hash;
        assert_eq!(0, addr.start);
        let path = self.path_for_file(hash);
        // TODO: Specific error for compression failure (corruption?) vs io errors.
        let (compressed_len, decompressed) = match read_and_decompress(&path) {
            Ok(d) => d,
            Err(e) => {
                report.increment("block.corrupt", 1);
                error!("Block file {:?} read error {:?}", path, e);
                return Err(ErrorKind::BlockCorrupt(hash.clone()).into());
            }
        };
        // TODO: Accept addresses referring to only part of a block.
        assert_eq!(decompressed.len(), addr.len as usize);
        report.increment("block", 1);
        report.increment_size("block", decompressed.len() as u64, compressed_len as u64);

        let actual_hash = blake2b::blake2b(BLAKE_HASH_SIZE_BYTES, &[], &decompressed)
            .as_bytes()
            .to_hex();
        if actual_hash != *hash {
            report.increment("block.misplaced", 1);
            error!("Block file {:?} has actual decompressed hash {:?}", path, actual_hash);
            return Err(ErrorKind::BlockCorrupt(hash.clone()).into());
        }
        Ok(decompressed)
    }
}


#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::SeekFrom;
    use std::io::prelude::*;
    use tempdir;
    use tempfile;
    use super::{BlockDir};
    use super::super::report::Report;

    const EXAMPLE_TEXT: &'static [u8] = b"hello!";
    const EXAMPLE_BLOCK_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee\
        31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    fn make_example_file() -> tempfile::NamedTempFile {
        let mut tf = tempfile::NamedTempFile::new().unwrap();
        tf.write_all(EXAMPLE_TEXT).unwrap();
        tf.flush().unwrap();
        tf.seek(SeekFrom::Start(0)).unwrap();
        tf
    }

    fn setup() -> (tempdir::TempDir, BlockDir) {
        let testdir = tempdir::TempDir::new("block_test").unwrap();
        let block_dir = BlockDir::new(testdir.path());
        (testdir, block_dir)
    }

    #[test]
    pub fn write_to_file() {
        let expected_hash = EXAMPLE_BLOCK_HASH.to_string();
        let report = Report::new();
        let (testdir, mut block_dir) = setup();
        let mut example_file = make_example_file();

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), false);

        let (refs, hash_hex) = block_dir.store(&mut example_file, &report).unwrap();
        assert_eq!(hash_hex, EXAMPLE_BLOCK_HASH);

        // Should be in one block, and as it's currently unsalted the hash is the same.
        assert_eq!(1, refs.len());
        assert_eq!(0, refs[0].start);
        assert_eq!(EXAMPLE_BLOCK_HASH, refs[0].hash);

        // Subdirectory and file should exist
        let expected_file = testdir.path().join("66a").join(EXAMPLE_BLOCK_HASH);
        let attr = fs::metadata(expected_file).unwrap();
        assert!(attr.is_file());

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), true);

        assert_eq!(report.borrow_counts().get_count("block.already_present"), 0);
        assert_eq!(report.borrow_counts().get_count("block"), 1);
        assert_eq!(report.borrow_counts().get_size("block"), (6, 10));

        // Try to read back
        let read_report = Report::new();
        assert_eq!(read_report.borrow_counts().get_count("block"), 0);
        let back = block_dir.get(&refs[0], &read_report).unwrap();
        assert_eq!(back, EXAMPLE_TEXT);
        {
            let counts = read_report.borrow_counts();
            assert_eq!(counts.get_count("block"), 1);
            assert_eq!(counts.get_size("block"), (EXAMPLE_TEXT.len() as u64, 10u64));
        }
    }

    #[test]
    pub fn write_same_data_again() {
        let report = Report::new();
        let (_testdir, mut block_dir) = setup();

        let mut example_file = make_example_file();
        let (refs1, hash1) = block_dir.store(&mut example_file, &report).unwrap();
        assert_eq!(report.borrow_counts().get_count("block.already_present"), 0);
        assert_eq!(report.borrow_counts().get_count("block"), 1);

        let mut example_file = make_example_file();
        let (refs2, hash2) = block_dir.store(&mut example_file, &report).unwrap();
        assert_eq!(report.borrow_counts().get_count("block.already_present"), 1);
        assert_eq!(report.borrow_counts().get_count("block"), 1);

        assert_eq!(hash1, hash2);
        assert_eq!(refs1, refs2);
    }
}
