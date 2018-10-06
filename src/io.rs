// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! IO utilities.

#[cfg(test)]
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use tempfile;

use super::*;

pub struct AtomicFile {
    path: PathBuf,
    f: tempfile::NamedTempFile,
}

impl AtomicFile {
    pub fn new(path: &Path) -> Result<AtomicFile> {
        let dir = path.parent().unwrap();
        Ok(AtomicFile {
            path: path.to_path_buf(),
            f: tempfile::NamedTempFileOptions::new()
                .prefix("tmp")
                .create_in(dir)?,
        })
    }

    pub fn close(self: AtomicFile, _report: &Report) -> Result<()> {
        // try!(report.measure_duration("sync", || self.f.sync_all()));
        // We use `persist` rather than `persist_noclobber` here because the latter calls
        // `link` on Unix, and some filesystems don't support it.  That's probably fine
        // because the files being updated by this should never already exist, though
        // it does mean we won't detect unexpected cases where it does.
        if let Err(e) = self.f.persist(&self.path) {
            return Err(e.error.into());
        };
        Ok(())
    }
}

impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.f.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.f.flush()
    }
}

impl Deref for AtomicFile {
    type Target = fs::File;

    fn deref(&self) -> &Self::Target {
        &self.f
    }
}

impl DerefMut for AtomicFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.f
    }
}

pub fn ensure_dir_exists(path: &Path) -> Result<()> {
    if let Err(e) = fs::create_dir(path) {
        if e.kind() != io::ErrorKind::AlreadyExists {
            return Err(e.into());
        }
    }
    Ok(())
}

/// True if path exists and is a directory, false if does not exist, error otherwise.
#[allow(dead_code)]
pub fn directory_exists(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                Ok(true)
            } else {
                Err("exists but not a directory".into())
            }
        }
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => Ok(false),
            _ => Err(e.into()),
        },
    }
}

/// True if path exists and is a file, false if does not exist, error otherwise.
pub fn file_exists(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_file() {
                Ok(true)
            } else {
                Err("exists but not a file".into())
            }
        }
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => Ok(false),
            _ => Err(e.into()),
        },
    }
}

/// List a directory.
///
/// Returns a set of filenames and a set of directory names respectively, forced to UTF-8.
#[cfg(test)] // Only from tests at the moment but could be more general.
pub fn list_dir(path: &Path) -> Result<(HashSet<String>, HashSet<String>)> {
    let mut file_names = HashSet::<String>::new();
    let mut dir_names = HashSet::<String>::new();
    for entry in fs::read_dir(path)? {
        let entry = entry.unwrap();
        let entry_filename = entry.file_name().into_string().unwrap();
        let entry_type = entry.file_type()?;
        if entry_type.is_file() {
            file_names.insert(entry_filename);
        } else if entry_type.is_dir() {
            dir_names.insert(entry_filename);
        } else {
            panic!("don't recognize file type of {:?}", entry_filename);
        }
    }
    Ok((file_names, dir_names))
}

/// Create a directory if it doesn't exist; if it does then assert it must be empty.
pub fn require_empty_directory(path: &Path) -> Result<()> {
    if let Err(e) = std::fs::create_dir(&path) {
        if e.kind() == io::ErrorKind::AlreadyExists {
            // Exists and hopefully empty?
            if std::fs::read_dir(&path)?.next().is_some() {
                Err(e).chain_err(|| format!("Directory exists and is not empty {:?}", path))
            } else {
                Ok(()) // Exists and empty
            }
        } else {
            Err(e).chain_err(|| format!("Failed to create directory {:?}", path))
        }
    } else {
        Ok(()) // Created
    }
}

#[cfg(test)]
mod tests {
    // TODO: Somehow test the error cases.
    // TODO: Specific test for write_compressed_bytes.
}
