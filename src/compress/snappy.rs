// Copyright 2017, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Snappy compression glue.

use bytes::{Bytes, BytesMut};
use snap::raw::{Decoder, Encoder};

use crate::Result;

pub(crate) struct Compressor {
    encoder: Encoder,
}

impl Compressor {
    pub fn new() -> Compressor {
        Compressor {
            encoder: Encoder::new(),
        }
    }

    /// Compress bytes into unframed Snappy data.
    pub fn compress(&mut self, input: &[u8]) -> Result<Bytes> {
        let max_len = snap::raw::max_compress_len(input.len());
        let mut out = BytesMut::zeroed(max_len);
        let actual_len = self.encoder.compress(input, &mut out)?;
        out.truncate(actual_len);
        Ok(out.freeze())
    }
}

#[derive(Default)]
pub(crate) struct Decompressor {
    decoder: Decoder,
}

impl Decompressor {
    pub fn new() -> Decompressor {
        Decompressor::default()
    }

    /// Decompressed unframed Snappy data.
    ///
    /// Returns a slice pointing into a reusable object inside the Decompressor.
    pub fn decompress(&mut self, input: &[u8]) -> Result<Bytes> {
        let max_len = snap::raw::decompress_len(input)?;
        let mut out = BytesMut::zeroed(max_len);
        let actual_len = self.decoder.decompress(input, &mut out)?;
        out.truncate(actual_len);
        Ok(out.freeze())
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
        assert_eq!(comp.as_ref(), b"\x0b(hello world");
        assert_eq!(
            decompressor.decompress(&comp).unwrap().as_ref(),
            b"hello world"
        );

        let long_input = b"hello world, hello world, hello world, hello world";
        let comp = compressor.compress(long_input).unwrap();
        assert_eq!(comp.as_ref(), b"\x32\x30hello world, \x92\x0d\0");
        assert_eq!(decompressor.decompress(&comp).unwrap(), &long_input[..]);
    }
}
