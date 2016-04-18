// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! IO utilities.

use std::fs::{File, remove_file};
use std::io;
use std::io::Write;
use std::path::{Path, };


/// Write bytes to a file, and close.  If writing fails, delete the file.
pub fn write_file_entire(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // TODO: Somehow test the error cases.
    let mut f = match File::create(path) {
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
