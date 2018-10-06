// Copyright 2017 Martin Pool.

/// Abstracted compression algorithms.
use std::io;

pub mod snappy;

pub trait Compression {
    fn compress_and_write(b: &[u8], w: &mut io::Write) -> io::Result<(usize)>;
    fn decompress_read(r: &mut io::Read) -> io::Result<(usize, Vec<u8>)>;
}
