// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Write body data to a data block, compressed, and stored by its hash.
//!
//! Blocks are required to be less than 1GB uncompressed, so they can be held
//! entirely in memory on a typical machine.

use std::io;
use std::io::Write;
use blake2_rfc::blake2b::Blake2b;
use brotli2::write::BrotliEncoder;

const BROTLI_COMPRESSION_LEVEL: u32 = 4;
const BLAKE_HASH_SIZE_BYTES: usize = 64;

/// Single-use writer to a data block.  Data is compressed and its hash is
/// accumulated until writing is complete.
///
/// TODO: Implement all of std::io::Write?
pub struct BlockWriter {
    encoder: BrotliEncoder<Vec<u8>>,
    hasher: Blake2b,
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
    
    #[test]
    pub fn test_simple_write_all() {
        let mut writer = BlockWriter::new();
        writer.write_all("hello!".as_bytes()).unwrap();
        let (compressed, hash) = writer.finish().unwrap();
        println!("Compressed result: {:?}", compressed);
        assert!(compressed.len() == 10);
        assert!(hash.len() == 64);
        
        // TODO: Test uncompressing?
        // TODO: Test hash is as expected.
    }
}
