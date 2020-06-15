// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2020 Martin Pool.

//! IO utilities.

use std::fs;
use std::io;
use std::path::Path;

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
