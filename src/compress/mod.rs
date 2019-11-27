// Copyright 2017, 2019 Martin Pool.

/// Abstracted compression algorithms.
use std::io;

pub mod snappy;
pub mod zstd;

pub trait Compression {
    fn compress_and_write(b: &[u8], w: &mut dyn io::Write) -> io::Result<(usize)>;
    fn decompress_read(r: &mut dyn io::Read) -> io::Result<(usize, Vec<u8>)>;
}
