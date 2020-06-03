// Copyright 2017, 2019, 2020 Martin Pool.

/// Snappy compression.
use std::io;
use std::path::Path;

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

pub fn decompress_file<P: AsRef<Path>>(p: P) -> io::Result<(usize, Vec<u8>)> {
    let buf = std::fs::read(p.as_ref())?;
    // TODO: Reuse decoder.
    Ok((buf.len(), snap::raw::Decoder::new().decompress_vec(&buf)?))
}
