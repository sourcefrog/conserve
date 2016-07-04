// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! IO utilities.

use brotli2;

use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, };

use tempfile;


/// Write bytes to a file, and close.
/// If writing fails, delete the file.
/// The file must not already exist.
pub fn write_file_entire(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap();
    let mut f = try!(tempfile::NamedTempFileOptions::new()
        .prefix("tmp").create_in(dir));
    if let Err(e) = f.write_all(bytes) {
        error!("Couldn't write {:?}: {}", path.display(), e);
        return Err(e)
    };
    try!(f.sync_all());
    if let Err(e) = f.persist_noclobber(path) {
        return Err(e.error);
    };
    Ok(())
}


pub fn read_and_decompress(path: &Path) -> io::Result<Vec<u8>> {
    let f = try!(fs::File::open(&path));
    let mut decoder = brotli2::read::BrotliDecoder::new(f);
    let mut decompressed = Vec::<u8>::new();
    try!(decoder.read_to_end(&mut decompressed));
    Ok(decompressed)
}


/// Compress some bytes and write to a new file.
///
/// Returns the length of compressed bytes written.
pub fn write_compressed_bytes(to_path: &Path, uncompressed: &[u8]) -> io::Result<(usize)> {
    let mut compressed = Vec::<u8>::with_capacity(uncompressed.len());
    let params = brotli2::stream::CompressParams::new();
    try!(brotli2::stream::compress_vec(&params, &uncompressed, &mut compressed));
    try!(write_file_entire(to_path, &compressed));
    Ok(compressed.len())
}


pub fn ensure_dir_exists(path: &Path) -> io::Result<()> {
    if let Err(e) = fs::create_dir(path) {
        if e.kind() != ErrorKind::AlreadyExists {
            return Err(e);
        }
    }
    Ok(())
}


/// List a directory.
///
/// Returns a set of filenames and a set of directory names respectively.
#[cfg(test)]
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
    use std::fs;
    use std::io;
    use tempdir;

    use super::write_file_entire;

    #[test]
    pub fn test_write_file_entire_repeated() {
        let tmp = tempdir::TempDir::new("write_new_file_test").unwrap();
        let testfile = tmp.path().join("afile");
        write_file_entire(&testfile, "hello".as_bytes()).unwrap();

        assert_eq!(write_file_entire(&testfile, "goodbye".as_bytes())
                   .unwrap_err().kind(),
                   io::ErrorKind::AlreadyExists);

        // Should not have been overwritten.
        assert_eq!(fs::metadata(&testfile).unwrap().len(),
            "hello".len() as u64);
    }

    // TODO: Somehow test the error cases.
    // TODO: Specific test for write_compressed_bytes.
}
