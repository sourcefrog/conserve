// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! IO utilities.

use std::fs::{OpenOptions, remove_file};
use std::io;
use std::io::{Write};
use std::path::{Path, };


/// Write bytes to a file, and close.
/// If writing fails, delete the file.
/// The file must not already exist.
///
/// NOTE: This requires Rust >= 1.9 for `OpenOptions::create_new`.
pub fn write_file_entire(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // TODO: Somehow test the error cases.
    let mut f = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(f) => f,
        Err(e) => {
            error!("Couldn't create {:?}: {}", path.display(), e);
            return Err(e);
        }
    };
    if let Err(e) = f.write_all(bytes) {
        error!("Couldn't write {:?}: {}", path.display(), e);
        drop(f);
        if let Err(remove_err) = remove_file(path) {
            error!("Couldn't remove {:?}: {}", path.display(), remove_err);
        }
        Err(e)
    } else {
        Ok(())
    }
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
}
