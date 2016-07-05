// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Archives holding backup material.
//!
//! Archives must be initialized before use, which creates the directory.
//!
//! Archives can contain a tree of bands, which themselves contain file versions.

use std;
use std::fs::{File, read_dir};
use std::io;
use std::io::{Error, ErrorKind, Result, Read};
use std::path::{Path, PathBuf} ;

use rustc_serialize::json;

use super::band::{Band, BandId};
use super::io::write_file_entire;


const HEADER_FILENAME: &'static str = "CONSERVE";
const ARCHIVE_VERSION: &'static str = "0.2.0";

#[derive(Debug)]
pub struct Archive {
    /// Top-level directory for the archive.
    path: PathBuf,
}

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct ArchiveHeader {
    conserve_archive_version: String,
}

impl Archive {
    /// Make a new directory to hold an archive, and write the header.
    pub fn init(path: &Path) -> Result<Archive> {
        debug!("Creating archive directory {:?}", path.display());
        let archive = Archive {
            path: path.to_path_buf(),
        };
        if let Err(e) = std::fs::create_dir(&archive.path) {
            error!("Failed to create archive directory {:?}: {}",
                archive.path.display(), e);
            return Err(e);
        };
        if let Err(e) = archive.write_archive_header() {
            error!("Failed to write archive header: {}", e);
            return Err(e)
        };
        info!("Created new archive in {:?}", path.display());
        Ok(archive)
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open(path: &Path) -> Result<Archive> {
        let archive = Archive {
            path: path.to_path_buf(),
        };
        let header_path = path.join(HEADER_FILENAME);
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
        let header_path = self.path.join(HEADER_FILENAME);
        let header_json = json::encode(&header).unwrap() + "\n";
        debug!("header json = {}", header_json);
        write_file_entire(&header_path, header_json.as_bytes())
    }

    /// Returns a vector of ids for bands currently present.
    pub fn list_bands(self: &Archive) -> Result<Vec<BandId>> {
        // TODO: Maybe make this an iterator?
        let mut band_ids = Vec::<BandId>::new();
        for entry_result in try!(read_dir(&self.path)) {
            let entry = try!(entry_result);
            if try!(entry.file_type()).is_dir() {
                if let Ok(name_string) = entry.file_name().into_string() {
                    if let Some(band_id) = BandId::from_string(&name_string) {
                        band_ids.push(band_id);
                    } else {
                        warn!("unexpected archive subdirectory {:?}", &name_string);
                    }
                } else {
                    warn!("unexpected archive subdirectory with un-decodable name {:?}",
                        entry.file_name())
                }
            }
        }
        Ok(band_ids)
    }

    /// Returns the top-level directory for the archive.
    ///
    /// The top-level directory contains a `CONSERVE` header file, and zero or more
    /// band directories.
    pub fn path(self: &Archive) -> &Path {
        self.path.as_path()
    }

    /// Make a new band. Bands are numbered sequentially.
    pub fn create_band(self: &Archive) -> io::Result<Band> {
        // TODO: Increment id if directory is not empty.
        Band::create(self.path(), BandId::zero())
    }
}


#[cfg(test)]
extern crate tempdir;

/// Makes an archive in a temporary directory, that will be deleted when it goes out of
/// scope.
//
// TODO: Merge with ArchiveFixture.
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

    use std::fs;
    use std::io::Read;

    use super::*;
    use super::super::band::BandId;
    use super::super::io::list_dir;

    #[test]
    fn create_then_open_archive() {
        let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
        let arch_path = &testdir.path().join("arch");
        let arch = Archive::init(arch_path).unwrap();

        assert_eq!(arch.path(), arch_path.as_path());
        assert!(arch.list_bands().unwrap().is_empty());

        // We can re-open it.
        Archive::open(arch_path).unwrap();
        assert!(arch.list_bands().unwrap().is_empty());
    }

    /// A new archive contains just one header file.
    /// The header is readable json containing only a version number.
    #[test]
    fn new_archive_header_contents() {
        let (_tempdir, arch) = scratch_archive();

        let (file_names, dir_names) = list_dir(arch.path()).unwrap();
        assert_eq!(file_names.len(), 1);
        assert!(file_names.contains("CONSERVE"));
        assert_eq!(dir_names.len(), 0);

        let header_path = arch.path().join("CONSERVE");
        let mut header_file = fs::File::open(&header_path).unwrap();
        let mut contents = String::new();
        header_file.read_to_string(&mut contents).unwrap();
        assert_eq!(
            contents,
            "{\"conserve_archive_version\":\"0.2.0\"}\n");
    }

    /// Can create bands in an archive.
    #[test]
    fn create_bands() {
        use super::super::io::directory_exists;
        let (_tempdir, arch) = scratch_archive();

        // Make one band
        let _band1 = arch.create_band().unwrap();
        assert!(directory_exists(arch.path()).unwrap());
        let (_file_names, dir_names) = list_dir(arch.path()).unwrap();
        println!("dirs: {:?}", dir_names);
        assert!(dir_names.contains("b0000"));

        assert_eq!(arch.list_bands().unwrap(),
            vec![BandId::new(&[0])]);

        // // Try creating a second band.
        // let _band2 = arch.create_band().unwrap();
        // let (_file_names, dir_names) = list_dir(arch.path()).unwrap();
        // println!("dirs: {:?}", dir_names);
        // assert_eq!(dir_names.len(), 2);

    }
}
