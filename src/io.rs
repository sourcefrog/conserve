// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! IO utilities.

use brotli2;

use std::fs;
use std::io;
use std::io::{Read, Write};
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
}
