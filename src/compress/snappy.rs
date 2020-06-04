// Copyright 2017, 2019, 2020 Martin Pool.

/// Snappy compression glue.
use std::io;
use std::path::Path;

use crate::Result;

/// Holds a reusable buffer for Snappy compression.
#[derive(Default)]
pub(crate) struct Compressor {
    out_buf: Vec<u8>,
}

impl Compressor {
    pub fn new() -> Compressor {
        Compressor::default()
    }

    /// Compress bytes into unframed Snappy data.
    ///
    /// Returns a slice referencing a buffer in this object, valid only
    /// until the next call.
    #[must_use]
    pub fn compress(&mut self, input: &[u8]) -> Result<&[u8]> {
        let max_len = snap::raw::max_compress_len(input.len());
        if self.out_buf.len() < max_len {
            self.out_buf.resize(max_len, 0u8);
        }
        let actual_len = snap::raw::Encoder::new().compress(input, &mut self.out_buf)?;
        Ok(&self.out_buf[0..actual_len])
    }
}

/// Read the complete contents of a file and decompress it.
///
/// Returns a tuple of the length of compressed data, and a Vec of the decompressed data.
/// The length of the vec is the length of decompressed data.
pub fn decompress_file<P: AsRef<Path>>(p: P) -> io::Result<(usize, Vec<u8>)> {
    let buf = std::fs::read(p.as_ref())?;
    // TODO: Reuse decoder.
    Ok((buf.len(), snap::raw::Decoder::new().decompress_vec(&buf)?))
}

/// Decompressed unframed Snappy data, from a slice into a vec.
///
/// On return, the length of the output vec is the length of uncompressed data.
#[allow(dead_code)]
fn decompress_bytes(input: &[u8], output: &mut Vec<u8>) -> Result<()> {
    // This is currently unused, but will be needed to access archives not on the
    // local filesystem.
    let len = snap::raw::decompress_len(input)?;
    output.resize(len, 0u8);
    let actual_len = snap::raw::Decoder::new().decompress(input, output)?;
    output.truncate(actual_len);
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn compressor() {
        let mut compressor = Compressor::new();
        assert_eq!(
            compressor.compress(b"hello world").unwrap(),
            b"\x0b(hello world"
        );
        assert_eq!(
            compressor
                .compress(b"hello world, hello world, hello world, hello world")
                .unwrap(),
            b"\x32\x30hello world, \x92\x0d\0"
        );
    }
}
