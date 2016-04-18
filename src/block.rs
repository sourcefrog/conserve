// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! File contents are stored in data blocks within an archive band.
//!
//! Blocks are required to be less than 1GB uncompressed, so they can be held
//! entirely in memory on a typical machine.

use std::fs::create_dir;
use std::io;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use blake2_rfc::blake2b::Blake2b;
use brotli2::write::BrotliEncoder;
use rustc_serialize::hex::ToHex;

use super::io::write_file_entire;

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
}


impl BlockWriter {
    /// Make a new BlockWriter, to write one block into a block data directory `dir`.
    pub fn new() -> BlockWriter {
        BlockWriter {
            encoder: BrotliEncoder::new(Vec::<u8>::new(), BROTLI_COMPRESSION_LEVEL),
            hasher: Blake2b::new(BLAKE_HASH_SIZE_BYTES),
        }
    }

    /// Write all the contents of `buf` into this block.
    ///
    /// If this returns an error then it's possible that the block was partly
    /// written, and the caller should discard it.
    pub fn write_all(self: &mut BlockWriter, buf: &[u8]) -> io::Result<()> {
        try!(self.encoder.write_all(buf));
        self.hasher.update(buf);
        Ok(())
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
    path: PathBuf,
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
    pub fn store(self: &BlockDir, bw: BlockWriter) -> io::Result<BlockHash> {
        let (compressed_bytes, hex_hash) = try!(bw.finish());
        let subdir = self.subdir_for(&hex_hash);
        if let Err(e) = create_dir(subdir) {
            if e.kind() != ErrorKind::AlreadyExists {
                return Err(e);
            }
        }
        try!(write_file_entire(&self.path_for_file(&hex_hash),
            compressed_bytes.as_slice()));
        Ok(hex_hash)
    }
}


#[cfg(test)]
mod tests {
    use super::{BlockDir,BlockWriter};
    use std::fs;
    use tempdir;
    
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
        assert_eq!(hash_hex,
            "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf21\
             45b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c");
    }
    
    #[test]
    pub fn test_write_to_file() {
        let testdir = tempdir::TempDir::new("block_test").unwrap();
        let mut writer = BlockWriter::new();
        let block_dir = BlockDir::new(testdir.path());
        
        writer.write_all("hello!".as_bytes()).unwrap();
        let hash_hex = block_dir.store(writer).unwrap();
        assert_eq!(hash_hex, EXAMPLE_BLOCK_HASH);
        
        // Subdirectory and file should exist
        let expected_file = testdir.path().join("66a").join(EXAMPLE_BLOCK_HASH);
        let attr = fs::metadata(expected_file).unwrap();
        assert!(attr.is_file());
    }

    // TODO: Try reading data back.
}
