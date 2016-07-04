// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! File contents are stored in data blocks within an archive band.
//!
//! Blocks are required to be less than 1GB uncompressed, so they can be held
//! entirely in memory on a typical machine.

use std::fs;
use std::io;
use std::io::prelude::*;
use std::io::{ErrorKind};
use std::path::{Path, PathBuf};

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use brotli2::write::{BrotliEncoder};
use rustc_serialize::hex::ToHex;

use super::io::{read_and_decompress, write_file_entire};
use super::report::Report;

/// Use a moderate Brotli compression level.
///
/// TODO: Is this a good tradeoff?
const BROTLI_COMPRESSION_LEVEL: u32 = 4;

/// Use the maximum 64-byte hash.
const BLAKE_HASH_SIZE_BYTES: usize = 64;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

/// The unique identifier for a block: its hexadecimal BLAKE2b hash.
pub type BlockHash = String;


/// Write body data to a data block, compressed, and stored by its hash.
///
/// A `BlockWriter` is a single-use object that writes a single block.
///
/// Data is compressed and its hash is
/// accumulated until writing is complete.
///
/// TODO: Implement all of std::io::Write?
pub struct BlockWriter {
    encoder: BrotliEncoder<Vec<u8>>,
    hasher: Blake2b,
    uncompressed_length: u64,
}


impl BlockWriter {
    /// Make a new BlockWriter, to write one block into a block data directory `dir`.
    pub fn new() -> BlockWriter {
        BlockWriter {
            encoder: BrotliEncoder::new(Vec::<u8>::new(), BROTLI_COMPRESSION_LEVEL),
            hasher: Blake2b::new(BLAKE_HASH_SIZE_BYTES),
            uncompressed_length: 0,
        }
    }

    /// Write all the contents of `buf` into this block.
    ///
    /// If this returns an error then it's possible that the block was partly
    /// written, and the caller should discard it.
    pub fn write_all(self: &mut BlockWriter, buf: &[u8]) -> io::Result<()> {
        try!(self.encoder.write_all(buf));
        self.uncompressed_length += buf.len() as u64;
        self.hasher.update(buf);
        Ok(())
    }

    pub fn copy_from_file(self: &mut BlockWriter, from_file: &mut fs::File) -> io::Result<()> {
        // TODO: Don't read the whole thing in one go, use smaller buffers to cope with
        // large files.
        let mut body = Vec::<u8>::new();
        try!(from_file.read_to_end(&mut body));
        self.write_all(&body)
    }

    /// Finish compression, and return the compressed bytes and a hex hash.
    ///
    /// Callers normally want `BlockDir.store` instead, which will
    /// finish and consume the writer.
    pub fn finish(self: BlockWriter) -> io::Result<(Vec<u8>, BlockHash)> {
        Ok((try!(self.encoder.finish()),
            self.hasher.finalize().as_bytes().to_hex()))
    }
}


/// A readable, writable directory within a band holding data blocks.
pub struct BlockDir {
    pub path: PathBuf,
}

fn block_name_to_subdirectory(block_hash: &str) -> &str {
    &block_hash[..SUBDIR_NAME_CHARS]
}

impl BlockDir {
    /// Create a BlockDir accessing `path`, which must exist as a directory.
    pub fn new(path: &Path) -> BlockDir {
        BlockDir {
            path: path.to_path_buf(),
        }
    }

    /// Return the subdirectory in which we'd put a file called `hash_hex`.
    fn subdir_for(self: &BlockDir, hash_hex: &BlockHash) -> PathBuf {
        let mut buf = self.path.clone();
        buf.push(block_name_to_subdirectory(hash_hex));
        buf
    }

    /// Return the full path for a file called `hex_hash`.
    fn path_for_file(self: &BlockDir, hash_hex: &BlockHash) -> PathBuf {
        let mut buf = self.subdir_for(hash_hex);
        buf.push(hash_hex);
        buf
    }

    /// Finish and store the contents of a BlockWriter.
    ///
    /// Returns the hex hash of the block.
    pub fn store(self: &BlockDir, bw: BlockWriter, report: &mut Report) -> io::Result<BlockHash> {
        report.increment("block.write.uncompressed_bytes", bw.uncompressed_length);
        let (compressed_bytes, hex_hash) = try!(bw.finish());
        if try!(self.contains(&hex_hash)) {
            report.increment("block.write.already_present", 1);
            return Ok(hex_hash);
        }
        let subdir = self.subdir_for(&hex_hash);
        try!(super::io::ensure_dir_exists(&subdir));
        if let Err(e) = write_file_entire(&self.path_for_file(&hex_hash),
            compressed_bytes.as_slice()) {
            if e.kind() == ErrorKind::AlreadyExists {
                // Suprising we saw this rather than detecting it above.
                warn!("Unexpected late detection of existing block {:?}", hex_hash);
                report.increment("block.write.already_present", 1);
            } else {
                return Err(e);
            }
        }
        report.increment("block.write.count", 1);
        report.increment("block.write.compressed_bytes", compressed_bytes.len() as u64);
        Ok(hex_hash)
    }

    /// True if the named block is present in this directory.
    pub fn contains(self: &BlockDir, hash: &BlockHash) -> io::Result<bool> {
        match fs::metadata(self.path_for_file(hash)) {
            Err(ref e) if e.kind() == ErrorKind::NotFound => Ok(false),
            Ok(_) => Ok(true),
            Err(e) => Err(e),
        }
    }

    /// Read back the contents of a block, as a byte array.
    pub fn get(self: &BlockDir, hash: &BlockHash, report: &mut Report) -> io::Result<Vec<u8>> {
        let path = self.path_for_file(hash);
        let decompressed = match read_and_decompress(&path) {
            Ok(d) => d,
            Err(e) => {
                error!("Block file {:?} couldn't be decompressed: {:?}", path, e);
                report.increment("block.read.corrupt", 1);
                return Err(e);
            }
        };
        report.increment("block.read", 1);

        let actual_hash = blake2b::blake2b(BLAKE_HASH_SIZE_BYTES, &[], &decompressed)
            .as_bytes().to_hex();
        if actual_hash != *hash {
            report.increment("block.read.misplaced", 1);
            error!("Block file {:?} has actual decompressed hash {:?}",
                path, actual_hash);
            return Err(io::Error::new(ErrorKind::InvalidData, "block.read.misplaced"));
        }
        return Ok(decompressed);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempdir;
    use super::{BlockDir, BlockWriter};
    use super::super::report::Report;

    const EXAMPLE_TEXT: &'static str = "hello!";
    const EXAMPLE_BLOCK_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf21\
         45b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    #[test]
    pub fn test_write_all_to_memory() {
        let mut writer = BlockWriter::new();

        writer.write_all("hello!".as_bytes()).unwrap();
        let (compressed, hash_hex) = writer.finish().unwrap();
        println!("Compressed result: {:?}", compressed);
        assert!(compressed.len() == 10);
        assert!(hash_hex.len() == 128);
        assert_eq!(hash_hex, EXAMPLE_BLOCK_HASH);
    }

    fn setup() -> (tempdir::TempDir, BlockDir) {
        let testdir = tempdir::TempDir::new("block_test").unwrap();
        let block_dir = BlockDir::new(testdir.path());
        return (testdir, block_dir);
    }

    #[test]
    pub fn test_write_to_file() {
        let mut writer = BlockWriter::new();
        let expected_hash = EXAMPLE_BLOCK_HASH.to_string();
        let mut report = Report::new();
        let (testdir, block_dir) = setup();

        assert_eq!(block_dir.contains(&expected_hash).unwrap(),
            false);

        writer.write_all(EXAMPLE_TEXT.as_bytes()).unwrap();
        let hash_hex = block_dir.store(writer, &mut report).unwrap();
        assert_eq!(hash_hex, EXAMPLE_BLOCK_HASH);

        // Subdirectory and file should exist
        let expected_file = testdir.path().join("66a").join(EXAMPLE_BLOCK_HASH);
        let attr = fs::metadata(expected_file).unwrap();
        assert!(attr.is_file());

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), true);

        assert_eq!(report.get_count("block.write.already_present"), 0);
        assert_eq!(report.get_count("block.write.count"), 1);
        assert_eq!(report.get_count("block.write.compressed_bytes"), 10);

        // Try to read back
        assert_eq!(report.get_count("block.read"), 0);
        let back = block_dir.get(&expected_hash, &mut report).unwrap();
        assert_eq!(back, EXAMPLE_TEXT.as_bytes());
        assert_eq!(report.get_count("block.read"), 1);
    }

    #[test]
    pub fn test_write_same_data_again() {
        let mut report = Report::new();
        let (_testdir, block_dir) = setup();

        let mut writer = BlockWriter::new();
        writer.write_all("hello!".as_bytes()).unwrap();
        let hash1 = block_dir.store(writer, &mut report).unwrap();
        assert_eq!(report.get_count("block.write.already_present"), 0);
        assert_eq!(report.get_count("block.write.count"), 1);

        let mut writer = BlockWriter::new();
        writer.write_all("hello!".as_bytes()).unwrap();
        let hash2 = block_dir.store(writer, &mut report).unwrap();
        assert_eq!(report.get_count("block.write.already_present"), 1);
        assert_eq!(report.get_count("block.write.count"), 1);

        assert_eq!(hash1, hash2);
    }
}
