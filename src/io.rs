// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! IO utilities.

#[cfg(test)]
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use brotli2;
use brotli2::write::BrotliEncoder;
use rustc_serialize::json;
use rustc_serialize;
use tempfile;

use super::Report;


struct AtomicFile {
    path: PathBuf,
    f: tempfile::NamedTempFile,
}

impl AtomicFile {
    fn new(path: &Path) -> io::Result<AtomicFile> {
        let dir = path.parent().unwrap();
        Ok(AtomicFile {
            path: path.to_path_buf(),
            f: try!(tempfile::NamedTempFileOptions::new().prefix("tmp").create_in(dir)),
        })
    }

    fn close(self: AtomicFile, report: &mut Report) -> io::Result<()> {
        if cfg!(feature = "sync") {
            try!(report.measure_duration("sync", || self.f.sync_all()));
        }
        if let Err(e) = self.f.persist_noclobber(&self.path) {
            return Err(e.error);
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


#[allow(unused)]
pub fn read_and_decompress(path: &Path) -> io::Result<Vec<u8>> {
    let f = try!(fs::File::open(&path));
    let mut decoder = brotli2::read::BrotliDecoder::new(f);
    let mut decompressed = Vec::<u8>::new();
    try!(decoder.read_to_end(&mut decompressed));
    Ok(decompressed)
}


pub fn write_json_uncompressed<T: rustc_serialize::Encodable>(
    path: &Path, obj: &T, report: &mut Report) -> io::Result<()> {
    let mut f = try!(AtomicFile::new(path));
    try!(f.write_all(json::encode(&obj).unwrap().as_bytes()));
    try!(f.write_all(b"\n"));
    try!(f.close(report));
    Ok(())
}


/// Compress some bytes and write to a new file.
///
/// Returns the length of compressed bytes written.
pub fn write_json_compressed<T: rustc_serialize::Encodable>(to_path: &Path, obj: &T, report: &mut Report)
-> io::Result<(u64, u64)> {
    let json_string = json::encode(obj).unwrap();
    let uncompressed_len = json_string.len() as u64;

    let af = try!(AtomicFile::new(to_path));
    let mut encoder = BrotliEncoder::new(af, super::BROTLI_COMPRESSION_LEVEL);
    try!(encoder.write_all(json_string.as_bytes()));

    let mut af = try!(encoder.finish());
    let compressed_len: u64 = try!(af.seek(SeekFrom::Current(0)));
    try!(af.close(report));
    Ok((uncompressed_len, compressed_len))
}


pub fn ensure_dir_exists(path: &Path) -> io::Result<()> {
    if let Err(e) = fs::create_dir(path) {
        if e.kind() != ErrorKind::AlreadyExists {
            return Err(e);
        }
    }
    Ok(())
}


/// True if path exists and is a directory, false if does not exist, error otherwise.
pub fn directory_exists(path: &Path) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                Ok(true)
            } else {
                Err(io::Error::new(io::ErrorKind::AlreadyExists, "exists but not a directory"))
            }
        },
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => Ok(false),
            _ => Err(e),
        }
    }
}


/// True if path exists and is a file, false if does not exist, error otherwise.
pub fn file_exists(path: &Path) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_file() {
                Ok(true)
            } else {
                Err(io::Error::new(io::ErrorKind::AlreadyExists, "exists but not a file"))
            }
        },
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => Ok(false),
            _ => Err(e),
        }
    }
}


/// List a directory.
///
/// Returns a set of filenames and a set of directory names respectively, forced to UTF-8.
#[cfg(test)] // Only from tests at the moment but could be more general.
pub fn list_dir(path: &Path) -> io::Result<(HashSet<String>, HashSet<String>)>
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
