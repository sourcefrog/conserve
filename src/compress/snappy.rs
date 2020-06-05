// Copyright 2017, 2019, 2020 Martin Pool.

//! Snappy compression glue.

use snap::raw::{Decoder, Encoder};

use crate::Result;

/// Holds a reusable buffer for Snappy compression.
pub(crate) struct Compressor {
    out_buf: Vec<u8>,
    encoder: Encoder,
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
        let actual_len = self.encoder.compress(input, &mut self.out_buf)?;
        Ok(&self.out_buf[0..actual_len])
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Compressor {
            out_buf: Vec::new(),
            encoder: Encoder::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Decompressor {
    out_buf: Vec<u8>,
    decoder: Decoder,
}

impl Decompressor {
    pub fn new() -> Decompressor {
        Decompressor::default()
    }

    /// Decompressed unframed Snappy data, from a slice into a vec.
    ///
    /// On return, the length of the output vec is the length of uncompressed data.
    pub fn decompress(&mut self, input: &[u8]) -> Result<&[u8]> {
        // This is currently unused, but will be needed to access archives not on the
        // local filesystem.
        let len = snap::raw::decompress_len(input)?;
        if self.out_buf.len() < len {
            self.out_buf.resize(len, 0u8);
        }
        let actual_len = self.decoder.decompress(input, &mut self.out_buf)?;
        Ok(&self.out_buf[..actual_len])
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn compressor_decompressor() {
        let mut compressor = Compressor::new();
        let mut decompressor = Decompressor::new();

        let comp = compressor.compress(b"hello world").unwrap();
        assert_eq!(comp, b"\x0b(hello world");
        assert_eq!(decompressor.decompress(&comp).unwrap(), b"hello world");

        let long_input = b"hello world, hello world, hello world, hello world";
        let comp = compressor.compress(long_input).unwrap();
        assert_eq!(comp, b"\x32\x30hello world, \x92\x0d\0");
        assert_eq!(decompressor.decompress(&comp).unwrap(), &long_input[..]);
    }
}
