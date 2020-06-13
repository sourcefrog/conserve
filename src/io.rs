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

/// List a directory.
///
/// Returns a list of filenames and a list of directory names respectively, forced to UTF-8, and
/// sorted naively as UTF-8.
#[cfg(test)]
pub fn list_dir(path: &Path) -> std::io::Result<(Vec<String>, Vec<String>)> {
    // TODO: Replace use of this in tests by assert_fs.
    let mut file_names = Vec::<String>::new();
    let mut dir_names = Vec::<String>::new();
    for entry in fs::read_dir(path)? {
        let entry = entry.unwrap();
        let entry_filename = entry.file_name().into_string().unwrap();
        let entry_type = entry.file_type()?;
        if entry_type.is_file() {
            file_names.push(entry_filename);
        } else if entry_type.is_dir() {
            dir_names.push(entry_filename);
        } else {
            // TODO: Don't panic, just warn?
            panic!("don't recognize file type of {:?}", entry_filename);
        }
    }
    file_names.sort_unstable();
    dir_names.sort_unstable();
    Ok((file_names, dir_names))
}
