// Copyright 2017, 2019 Martin Pool.

/// Abstracted compression algorithms.
use std::io;

pub mod snappy;

pub trait Compression {
    fn compress_and_write(b: &[u8], w: &mut dyn io::Write) -> io::Result<usize>;
}
