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
use std::io::{Error, ErrorKind, Result, Read};
use std::path::{Path, PathBuf};

use rustc_serialize::json;

use super::{ARCHIVE_VERSION, Band, BandId, Report};
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
        let mut report = Report::new();
        if let Err(e) = std::fs::create_dir(&archive.path) {
            error!("Failed to create archive directory {:?}: {}",
                   archive.path.display(),
                   e);
            return Err(e);
        };
        let header = ArchiveHeader { conserve_archive_version: String::from(ARCHIVE_VERSION) };
        if let Err(e) = write_json_uncompressed(&path.join(HEADER_FILENAME), &header,
            &mut report) {
            error!("Failed to write archive header: {}", e);
            return Err(e);
        };
        info!("Created new archive in {:?}", path.display());
        Ok(archive)
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open(path: &Path) -> Result<Archive> {
        let archive = Archive { path: path.to_path_buf() };
        let header_path = path.join(HEADER_FILENAME);
        let mut header_file = try!(File::open(&header_path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                error!("{} is not a Conserve archive", path.as_os_str().to_string_lossy());
            } else {
                error!("Couldn't open archive header {:?}: {}",
                        header_path.display(),
                        e);
            }
            e
        }));

        let mut header_string = String::new();
        if let Err(e) = header_file.read_to_string(&mut header_string) {
            error!("Failed to read archive header {:?}: {}", header_file, e);
            return Err(e);
        }

        let header: ArchiveHeader = try!(json::decode(&header_string).map_err(|e| {
            error!("Couldn't deserialize archive header: {}", e);
            Error::new(ErrorKind::InvalidInput, e)
        }));

        if header.conserve_archive_version != ARCHIVE_VERSION {
            error!("Wrong archive version in header {:?}: {:?}",
                   header,
                   header.conserve_archive_version);
            return Err(Error::new(ErrorKind::InvalidInput, header.conserve_archive_version));
        }
        Ok(archive)
    }

    /// Returns a iterator of ids for bands currently present, in arbitrary order.
    pub fn iter_bands(self: &Archive) -> Result<IterBands> {
        let read_dir = match read_dir(&self.path) {
            Ok(r) => r,
            Err(e) => {
                error!("{:?} reading directory {:?}", e, &self.path);
                return Err(e);
            }
        };
        Ok(IterBands {
            dir_iter: read_dir,
            path: self.path.clone(),
        })
    }

    /// Returns a vector of band ids, in sorted order.
    pub fn list_bands(self: &Archive) -> Result<Vec<BandId>> {
        let mut band_ids: Vec<_> = try!(try!(self.iter_bands()).collect());
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
    pub fn last_band_id(self: &Archive) -> io::Result<Option<BandId>> {
        let max = try!(self.list_bands()).pop();
        Ok(max)
    }

    /// Make a new band. Bands are numbered sequentially.
    pub fn create_band(self: &Archive, mut report: &mut Report) -> io::Result<Band> {
        let new_band_id = match self.last_band_id() {
            Err(e) => return Err(e),
            Ok(None) => BandId::zero(),
            Ok(Some(b)) => b.next_sibling(),
        };
        Band::create(self.path(), new_band_id, &mut report)
    }

    pub fn open_band(&self, band_id: &BandId, report: &mut Report) -> io::Result<Band> {
        Band::open(self.path(), band_id, report)
    }
}


pub struct IterBands {
    dir_iter: fs::ReadDir,
    path: PathBuf,
}


impl Iterator for IterBands {
    type Item = Result<BandId>;

    fn next(&mut self) -> Option<Result<BandId>> {
        loop {
            let entry = match self.dir_iter.next() {
                Some(Ok(entry)) => entry,
                Some(Err(e)) => {
                    error!("%{:?} reading directory entry from {:?}", e, self.path);
                    return Some(Err(e));
                }
                None => return None,
            };
            let ft = match entry.file_type() {
                Err(e) => {
                    error!("%{:?} reading directory entry from {:?}", e, self.path);
                    return Some(Err(e));
                }
                Ok(ft) => ft,
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
        let _band1 = af.create_band(&mut Report::new()).unwrap();
        assert!(directory_exists(af.path()).unwrap());
        let (_file_names, dir_names) = list_dir(af.path()).unwrap();
        println!("dirs: {:?}", dir_names);
        assert!(dir_names.contains("b0000"));

        assert_eq!(af.list_bands().unwrap(), vec![BandId::new(&[0])]);

        // // Try creating a second band.
        let _band2 = &af.create_band(&mut Report::new()).unwrap();
        assert_eq!(af.list_bands().unwrap(),
                   vec![BandId::new(&[0]), BandId::new(&[1])]);
    }
}
