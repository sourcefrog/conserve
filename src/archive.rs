// Conserve backup system.
// Copyright 2015 Martin Pool.

//! Archives holding backup material.
//!
//! Archives must be initialized before use, which creates the directory.
//!
//! Archives can contain a tree of bands, which themselves contain file versions.

use std;
use std::fs::{File};
use std::io::{Error, ErrorKind, Result, Read, Write};
use std::path::{Path, PathBuf} ;

use rustc_serialize::json;

use super::BandId;

const HEADER_FILENAME: &'static str = "CONSERVE";
const ARCHIVE_VERSION: &'static str = "0.2.0";

#[derive(Debug)]
pub struct Archive {
    /// Top-level directory for the archive.
    dir: PathBuf,
}

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct ArchiveHeader {
    conserve_archive_version: String,
}

impl Archive {
    /// Make a new directory to hold an archive, and write the header.
    pub fn init(dir: &Path) -> Result<Archive> {
        debug!("Creating archive directory {:?}", dir.display());
        let archive = Archive {
            dir: dir.to_path_buf(),
        };
        if let Err(e) = std::fs::create_dir(&archive.dir) {
            error!("Failed to create archive directory {:?}: {}",
                archive.dir.display(), e);
            return Err(e);
        };
        if let Err(e) = archive.write_archive_header() {
            error!("Failed to write archive header: {}", e);
            return Err(e)
        };
        info!("Created new archive in {:?}", dir.display());
        Ok(archive)
    }
    
    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open(dir: &Path) -> Result<Archive> {
        let archive = Archive {
            dir: dir.to_path_buf(),
        };
        let header_path = dir.join(HEADER_FILENAME);
        let mut header_file = match File::open(header_path.as_path()) {
            Ok(f) => f,
            Err(e) => {
                error!("Couldn't open archive header {:?}: {}",
                    header_path.display(), e);
                return Err(e);
            }
        };
        let mut header_string = String::new();
        if let Err(e) = header_file.read_to_string(&mut header_string) {
            error!("Failed to read archive header {:?}: {}",
                header_file, e);
            return Err(e);
        }
        let header: ArchiveHeader = match json::decode(&header_string) {
            Ok(h) => h,
            Err(e) => {
                error!("Couldn't deserialize archive header: {}", e);
                return Err(Error::new(ErrorKind::InvalidInput, e));
            }
        };
        if header.conserve_archive_version != String::from(ARCHIVE_VERSION) {
            error!("Wrong archive version in header {:?}: {:?}",
                header, header.conserve_archive_version);
            return Err(Error::new(ErrorKind::InvalidInput, header.conserve_archive_version));
        }
        Ok(archive)
    }

    fn write_archive_header(self: &Archive) -> Result<()> {
        let header = ArchiveHeader{
            conserve_archive_version: String::from(ARCHIVE_VERSION),
        };
        let header_path = self.dir.join(HEADER_FILENAME);
        let mut header_file = match File::create(&header_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Couldn't open archive header {:?}: {}",
                    header_path.display(), e);
                return Err(e)
            }
        };
        let header_json = json::encode(&header).unwrap() + "\n";
        debug!("header json = {}", header_json);
        if let Err(e) = header_file.write_all(header_json.as_bytes()) {
            error!("Couldn't write header file {:?}: {}",
                header_path.display(), e);
            return Err(e)
        }
        Ok(())
    }
    
    /// Returns a vector of ids for bands currently present.
    pub fn list_bands(self: &Archive) -> Result<Vec<BandId>> {
        Ok(Vec::new())
    }

    /// Returns the top-level directory for the archive.
    ///
    /// The top-level directory contains a `CONSERVE` header file, and zero or more
    /// band directories.
    pub fn path(self: &Archive) -> &Path {
        self.dir.as_path()
    }
}


#[cfg(test)]
extern crate tempdir;

/// Makes an archive in a temporary directory, that will be deleted when it goes out of
/// scope.
#[cfg(test)]
pub fn scratch_archive() -> (tempdir::TempDir, Archive) {
    let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
    let arch_path = &testdir.path().join("arch");
    let arch = Archive::init(arch_path).unwrap();
    (testdir, arch)
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::*;

    #[test]
    fn test_create_then_open_archive() {
        let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
        let arch_path = &testdir.path().join("arch");
        let arch = Archive::init(arch_path).unwrap();

        assert_eq!(arch.path(), arch_path.as_path());
        
        // We can re-open it.
        Archive::open(arch_path).unwrap();
    }
    
    #[test]
    fn test_new_archive_has_no_bands() {
        let (_tempdir, arch) = scratch_archive();
        assert!(arch.list_bands().unwrap().is_empty());
        panic!();
    }
}
