// Copyright 2019 Martin Pool.

/// zstd compression for Conserve.
use std::io;

use zstd;

pub struct Zstd {}

impl super::Compression for Zstd {
    fn compress_and_write(in_buf: &[u8], w: &mut dyn io::Write) -> io::Result<(usize)> {
        zstd::stream::copy_encode(in_buf, w, 0)?;
        Ok(in_buf.len())
    }

    fn decompress_read(r: &mut dyn io::Read) -> io::Result<(usize, Vec<u8>)> {
        // TODO: Maybe don't read to a buffer: instead count bytes read as we read, or just don't
        // bother returning the compressed byte count?

        // Conserve files are never too large so can always be read entirely in to memory.
        let mut compressed_buf = Vec::<u8>::with_capacity(10 << 20);
        let compressed_len = r.read_to_end(&mut compressed_buf)?;

        let decompressed = zstd::stream::decode_all(compressed_buf.as_slice())?;

        Ok((compressed_len, decompressed))
    }
}
