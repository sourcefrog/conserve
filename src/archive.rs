// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Archives holding backup material.
//!
//! Archives must be initialized before use, which creates the directory.
//!
//! Archives can contain a tree of bands, which themselves contain file versions.

use std;
use std::fs;
use std::fs::{File, read_dir};
use std::io;
use std::io::{Read};
use std::path::{Path, PathBuf};

use rustc_serialize::json;

use super::{ARCHIVE_VERSION, Band, BandId, Report};
use super::errors::*;
use super::io::write_json_uncompressed;


const HEADER_FILENAME: &'static str = "CONSERVE";

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
        let archive = Archive { path: path.to_path_buf() };
        // Report is not consumed because the results for init aren't so interesting.
        let report = Report::new();
        if let Err(e) = std::fs::create_dir(&archive.path) {
            if e.kind() == io::ErrorKind::AlreadyExists {
                // Exists and hopefully empty?
                if try!(std::fs::read_dir(&archive.path)).next().is_some() {
                    return Err(e).chain_err(|| format!("Archive directory exists and is not empty {:?}",
                        archive.path));
                }
            } else {
                return Err(e).chain_err(|| format!("Failed to create archive directory {:?}",
                    archive.path));
            }
        }
        let header = ArchiveHeader { conserve_archive_version: String::from(ARCHIVE_VERSION) };
        let header_filename = path.join(HEADER_FILENAME);
        try!(write_json_uncompressed(&header_filename, &header, &report)
            .chain_err(|| format!("Failed to write archive header: {:?}", header_filename)));
        info!("Created new archive in {:?}", path.display());
        Ok(archive)
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open(path: &Path) -> Result<Archive> {
        let archive = Archive { path: path.to_path_buf() };
        let header_path = path.join(HEADER_FILENAME);
        let mut header_file = match File::open(&header_path) {
            Ok(f) => f,
            Err(e) => {
                if e.kind() == io::ErrorKind::NotFound {
                    return Err(ErrorKind::NotAnArchive(path.into()).into());
                } else {
                    return Err(e.into());
                }
            }
        };
        let mut header_string = String::new();
        try!(header_file.read_to_string(&mut header_string));
        let header: ArchiveHeader = try!(json::decode(&header_string));
        if header.conserve_archive_version != ARCHIVE_VERSION {
            return Err(ErrorKind::UnsupportedArchiveVersion(header.conserve_archive_version).into());
        }
        Ok(archive)
    }

    /// Returns a iterator of ids for bands currently present, in arbitrary order.
    pub fn iter_bands(self: &Archive) -> Result<IterBands> {
        let read_dir = try!(read_dir(&self.path)
            .chain_err(|| format!("failed reading directory {:?}", &self.path)));
        Ok(IterBands {
            dir_iter: read_dir,
        })
    }

    /// Returns a vector of band ids, in sorted order.
    pub fn list_bands(self: &Archive) -> Result<Vec<BandId>> {
        let mut band_ids = Vec::<BandId>::new();
        for r in try!(self.iter_bands()) {
            band_ids.push(try!(r));
        }
        band_ids.sort();
        Ok(band_ids)
    }

    /// Returns the top-level directory for the archive.
    ///
    /// The top-level directory contains a `CONSERVE` header file, and zero or more
    /// band directories.
    pub fn path(self: &Archive) -> &Path {
        self.path.as_path()
    }

    // Return the id of the highest-numbered band, or None if empty.
    pub fn last_band_id(self: &Archive) -> Result<Option<BandId>> {
        Ok(try!(self.list_bands()).pop())
    }

    /// Make a new band. Bands are numbered sequentially.
    pub fn create_band(self: &Archive, report: &Report) -> Result<Band> {
        let new_band_id = match try!(self.last_band_id()) {
            None => BandId::zero(),
            Some(b) => b.next_sibling(),
        };
        Band::create(self.path(), new_band_id, report)
    }

    pub fn open_band(&self, band_id: &BandId, report: &Report) -> Result<Band> {
        Band::open(self.path(), band_id, report)
    }
}


pub struct IterBands {
    dir_iter: fs::ReadDir,
}


impl Iterator for IterBands {
    type Item = Result<BandId>;

    fn next(&mut self) -> Option<Result<BandId>> {
        loop {
            let entry = match self.dir_iter.next() {
                None => return None,
                Some(Ok(entry)) => entry,
                Some(Err(e)) => {
                    return Some(Err(e.into()));
                },
            };
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(e) => {
                    return Some(Err(e.into()));
                }
            };
            if !ft.is_dir() {
                continue;
            }
            if let Ok(name_string) = entry.file_name().into_string() {
                if let Some(band_id) = BandId::from_string(&name_string) {
                    return Some(Ok(band_id));
                } else {
                    warn!("unexpected archive subdirectory {:?}", &name_string);
                }
            } else {
                warn!("unexpected archive subdirectory with un-decodable name {:?}",
                      entry.file_name())
            }
        }
    }
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use std::fs;
    use std::io::Read;

    use super::*;
    use super::super::{BandId, Report};
    use super::super::io::list_dir;
    use super::super::testfixtures::ScratchArchive;

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

    #[test]
    fn init_empty_dir() {
        let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
        let arch_path = testdir.path();
        let arch = Archive::init(arch_path).unwrap();

        assert_eq!(arch.path(), arch_path);
        assert!(arch.list_bands().unwrap().is_empty());

        Archive::open(arch_path).unwrap();
        assert!(arch.list_bands().unwrap().is_empty());
    }


    /// A new archive contains just one header file.
    /// The header is readable json containing only a version number.
    #[test]
    fn new_archive_header_contents() {
        let af = ScratchArchive::new();
        let (file_names, dir_names) = list_dir(af.path()).unwrap();
        assert_eq!(file_names.len(), 1);
        assert!(file_names.contains("CONSERVE"));
        assert_eq!(dir_names.len(), 0);

        let header_path = af.path().join("CONSERVE");
        let mut header_file = fs::File::open(&header_path).unwrap();
        let mut contents = String::new();
        header_file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "{\"conserve_archive_version\":\"0.3\"}\n");
    }

    /// Can create bands in an archive.
    #[test]
    fn create_bands() {
        use super::super::io::directory_exists;
        let af = ScratchArchive::new();
        // Make one band
        let _band1 = af.create_band(&Report::new()).unwrap();
        assert!(directory_exists(af.path()).unwrap());
        let (_file_names, dir_names) = list_dir(af.path()).unwrap();
        println!("dirs: {:?}", dir_names);
        assert!(dir_names.contains("b0000"));

        assert_eq!(af.list_bands().unwrap(), vec![BandId::new(&[0])]);

        // // Try creating a second band.
        let _band2 = &af.create_band(&Report::new()).unwrap();
        assert_eq!(af.list_bands().unwrap(),
                   vec![BandId::new(&[0]), BandId::new(&[1])]);
    }
}
