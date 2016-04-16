// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! File contents are stored in data blocks within an archive band.
//!
//! Blocks are required to be less than 1GB uncompressed, so they can be held
//! entirely in memory on a typical machine.

use std::io;
use std::io::Write;
use blake2_rfc::blake2b::Blake2b;
use brotli2::write::BrotliEncoder;

/// Use a moderate Brotli compression level.
///
/// TODO: Is this a good tradeoff?
const BROTLI_COMPRESSION_LEVEL: u32 = 4;

/// Use the maximum 64-byte hash.
const BLAKE_HASH_SIZE_BYTES: usize = 64;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;


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

pub fn block_name_to_subdirectory(block_hash: &str) -> &str {
    &block_hash[..SUBDIR_NAME_CHARS]
}
    
impl BlockWriter {
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
    
    /// Finish writing.
    ///
    /// Returns a vector containing all the compressed data, and a byte
    /// array of the hash.
    pub fn finish(self: BlockWriter) -> io::Result<(Vec<u8>, Vec<u8>)> {
        let compressed = try!(self.encoder.finish());
        Ok((compressed, self.hasher.finalize().as_bytes().to_vec()))
    }

}

#[cfg(test)]
mod tests {
    use super::BlockWriter;
    use rustc_serialize::hex::ToHex;
    
    const EXAMPLE_BLOCK_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf21\
         45b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";
         
    #[test]
    pub fn test_block_name_to_subdirectory() {
        assert_eq!(super::block_name_to_subdirectory(EXAMPLE_BLOCK_HASH),
            "66a");
    }
    
    #[test]
    pub fn test_simple_write_all() {
        let mut writer = BlockWriter::new();
        writer.write_all("hello!".as_bytes()).unwrap();
        let (compressed, hash) = writer.finish().unwrap();
        println!("Compressed result: {:?}", compressed);
        assert!(compressed.len() == 10);
        assert!(hash.len() == 64);
        assert_eq!(hash.to_hex(),
            "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf21\
             45b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c");
        
        // TODO: Test uncompressing?
    }
}
