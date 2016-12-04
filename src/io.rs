// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! IO utilities.

#[cfg(test)]
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use brotli2;
use rustc_serialize::json;
use rustc_serialize;
use tempfile;

use super::Report;
use super::errors::*;


pub struct AtomicFile {
    path: PathBuf,
    f: tempfile::NamedTempFile,
}

impl AtomicFile {
    pub fn new(path: &Path) -> Result<AtomicFile> {
        let dir = path.parent().unwrap();
        Ok(AtomicFile {
            path: path.to_path_buf(),
            f: try!(tempfile::NamedTempFileOptions::new().prefix("tmp").create_in(dir)),
        })
    }

    pub fn close(self: AtomicFile, report: &Report) -> Result<()> {
        if cfg!(feature = "sync") {
            try!(report.measure_duration("sync", || self.f.sync_all()));
        }
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


pub fn read_and_decompress(path: &Path) -> io::Result<(usize, Vec<u8>)> {
    // Conserve files are never too large so can always be read entirely in to memory.
    let mut compressed_buf = Vec::<u8>::with_capacity(10<<20);
    let read_len = {
        let mut f = try!(fs::File::open(&path));
        try!(f.read_to_end(&mut compressed_buf))
    };
    compressed_buf.truncate(read_len);
    compressed_buf.shrink_to_fit();
    let mut decomp = brotli2::stream::Decompress::new();
    let mut decompressed = Vec::<u8>::with_capacity(read_len * 4);
    let mut inp = compressed_buf.as_slice();
    loop {
        match decomp.decompress_vec(&mut inp, &mut decompressed) {
            Err(e) => panic!("Brotli decompress error: {:?}", e),
            Ok(brotli2::stream::Status::Finished) => break,
            Ok(brotli2::stream::Status::NeedOutput) => (),
            Ok(x) => panic!("Unexpected Brotli2 status {:?}", x),
        };
        decompressed.reserve(read_len * 4);
    }
    decompressed.shrink_to_fit();
    Ok((read_len, decompressed))
}


pub fn write_json_uncompressed<T: rustc_serialize::Encodable>(
    path: &Path, obj: &T, report: &Report) -> Result<()> {
    let mut f = try!(AtomicFile::new(path));
    try!(f.write_all(json::encode(&obj).unwrap().as_bytes()));
    try!(f.write_all(b"\n"));
    try!(f.close(report));
    Ok(())
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
        },
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => Ok(false),
            _ => Err(e.into()),
        }
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
        },
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => Ok(false),
            _ => Err(e.into()),
        }
    }
}


/// List a directory.
///
/// Returns a set of filenames and a set of directory names respectively, forced to UTF-8.
#[cfg(test)] // Only from tests at the moment but could be more general.
pub fn list_dir(path: &Path) -> Result<(HashSet<String>, HashSet<String>)>
{
    let mut file_names = HashSet::<String>::new();
    let mut dir_names = HashSet::<String>::new();
    for entry in try!(fs::read_dir(path)) {
        let entry = entry.unwrap();
        let entry_filename = entry.file_name().into_string().unwrap();
        let entry_type = try!(entry.file_type());
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


#[cfg(test)]
mod tests {
    // TODO: Somehow test the error cases.
    // TODO: Specific test for write_compressed_bytes.
}
