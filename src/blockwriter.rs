// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Write body data to a data block, compressed, and stored by its hash.
//!
//! Blocks are required to be not too big in their compressed form
//! to fit in memory on the machines
//! that are reading and writing them: say 1GB.

use std::io;
use std::io::Write;
use brotli2::write::BrotliEncoder;

const BROTLI_COMPRESSION_LEVEL: u32 = 4;

/// Single-use writer to a data block.  Data is compressed and its hash is
/// accumulated until writing is complete.
///
/// TODO: Implement all of std::io::Write?
pub struct BlockWriter {
    encoder: BrotliEncoder<Vec<u8>>,
}
    
impl BlockWriter {
    pub fn new() -> BlockWriter {
        BlockWriter {
            encoder: BrotliEncoder::new(Vec::<u8>::new(), BROTLI_COMPRESSION_LEVEL),
        }
    }
    
    pub fn write_all(self: &mut BlockWriter, buf: &[u8]) -> io::Result<()> {
        self.encoder.write_all(buf)
        // TODO: Hash it
    }
    
    /// Finish writing, and return a vector containing all the compressed
    /// data.
    ///
    /// TODO: Also return the hash here?
    pub fn finish(self: BlockWriter) -> io::Result<Vec<u8>> {
        self.encoder.finish()
    }

}

#[cfg(test)]
mod tests {
    use super::BlockWriter;
    
    #[test]
    pub fn test_simple_write_all() {
        let mut writer = BlockWriter::new();
        writer.write_all("hello!".as_bytes()).unwrap();
        let result = writer.finish().unwrap();
        println!("Compressed result: {:?}", result);
        assert!(result.len() == 10);
        
        // TODO: Test uncompressing?
    }
}
