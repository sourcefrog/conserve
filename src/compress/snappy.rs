// Copyright 2017, 2019, 2020 Martin Pool.

/// Snappy compression.
use std::io;
use std::path::Path;

use snap;

pub struct Snappy {}

impl super::Compression for Snappy {
    fn compress_and_write(in_buf: &[u8], w: &mut dyn io::Write) -> io::Result<usize> {
        let mut encoder = snap::Encoder::new();
        let r = encoder.compress_vec(in_buf).unwrap();
        w.write_all(&r)?;
        Ok(r.len())
    }
}

pub fn decompress_file<P: AsRef<Path>>(p: P) -> io::Result<(usize, Vec<u8>)> {
    let buf = std::fs::read(p.as_ref())?;
    // TODO: Pass back error from snap decoder.
    Ok((
        buf.len(),
        snap::Decoder::new().decompress_vec(&buf).unwrap(),
    ))
}
