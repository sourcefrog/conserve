// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

///! Write data to a data block, compressed, and stored by its hash.
///!
///! Blocks are required to be not too big in their compressed form
///! to fit in memory on the machines
///! that are reading and writing them: say 1GB.

use std::io;

#[derive(Debug)]
pub struct BlockWriter {
}

impl BlockWriter {
    pub fn new() -> BlockWriter {
        BlockWriter{}
    }
    
    // TODO: Implement all of std::io::Write?
    pub fn write(self: &mut BlockWriter, buf: &[u8]) -> io::Result<usize> {
        // TODO: Compress and save the compressed data
        // TODO: Hash it
        Ok(buf.len())
    }
    
    // TODO: Allow retrieving the hash when ready
}

#[cfg(test)]
mod tests {
    use super::BlockWriter;
    
    #[test]
    pub fn test_simple_write() {
        let mut writer = BlockWriter::new();
        assert_eq!(writer.write("hello!".as_bytes()).unwrap(), 6);
    }
}
