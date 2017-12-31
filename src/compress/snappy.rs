// Copyright 2017 Martin Pool.

/// Snappy compression.

use std::io;

use snap;

pub struct Snappy {}

impl super::Compression for Snappy {
    fn compress_and_write(in_buf: &[u8], w: &mut io::Write) -> io::Result<(usize)> {
        let mut encoder = snap::Encoder::new();
        let r = encoder.compress_vec(in_buf).unwrap();
        w.write_all(&r)?;
        Ok(r.len())
    }

    fn decompress_read(r: &mut io::Read) -> io::Result<(usize, Vec<u8>)> {
        // Conserve files are never too large so can always be read entirely in to memory.
        let mut compressed_buf = Vec::<u8>::with_capacity(10 << 20);
        let compressed_len = r.read_to_end(&mut compressed_buf)?;

        let mut decoder = snap::Decoder::new();
        // TODO: Clean errors.
        let decompressed = decoder.decompress_vec(&compressed_buf).unwrap();

        Ok((compressed_len, decompressed))
    }
}
