// Copyright 2017, 2019, 2020 Martin Pool.

/// Snappy compression glue.
use std::io;
use std::path::Path;

use crate::Result;

pub struct Snappy {}

impl super::Compression for Snappy {
    /// Returns the number of compressed bytes written.
    fn compress_and_write(in_buf: &[u8], w: &mut dyn io::Write) -> io::Result<usize> {
        // TODO: Try to reuse encoders.
        let mut encoder = snap::raw::Encoder::new();
        let r = encoder.compress_vec(in_buf)?;
        w.write_all(&r)?;
        Ok(r.len())
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
