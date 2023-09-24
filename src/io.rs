// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! IO utilities.

use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::Path;

use bytes::BytesMut;

pub(crate) fn ensure_dir_exists(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path).or_else(|e| {
        if e.kind() == io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(e)
        }
    })
}

/// True if a directory exists and is empty.
pub(crate) fn directory_is_empty(path: &Path) -> std::io::Result<bool> {
    Ok(std::fs::read_dir(path)?.next().is_none())
}

/// Read up to `len` bytes into a buffer, and resize the vec to the bytes read.
pub(crate) fn read_with_retries(len: usize, from_file: &mut dyn Read) -> std::io::Result<BytesMut> {
    // TODO: This could safely resize the buf without initializing, since it will be overwritten.
    let mut buf = BytesMut::zeroed(len);
    let mut bytes_read = 0;
    while bytes_read < len {
        let read_len = from_file.read(&mut buf[bytes_read..])?;
        if read_len == 0 {
            break;
        }
        bytes_read += read_len;
    }
    buf.truncate(bytes_read);
    Ok(buf)
}
