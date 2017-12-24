// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Archives holding backup material.
//!
//! Archives must be initialized before use, which creates the directory.
//!
//! Archives can contain a tree of bands, which themselves contain file versions.

use std;
use std::fs;
use std::fs::read_dir;
use std::io;
use std::path::{Path, PathBuf};

use super::*;
use super::io::file_exists;
use super::jsonio;


const HEADER_FILENAME: &'static str = "CONSERVE";

#[derive(Clone, Debug)]
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
        let archive = Archive { path: path.to_path_buf() };
        // Report is not consumed because the results for init aren't so interesting.
        let report = Report::new();
        if let Err(e) = std::fs::create_dir(&archive.path) {
            if e.kind() == io::ErrorKind::AlreadyExists {
                // Exists and hopefully empty?
                if try!(std::fs::read_dir(&archive.path)).next().is_some() {
                    return Err(e).chain_err(|| {
                        format!("Archive directory exists and is not empty {:?}",
                                archive.path)
                    });
                }
            } else {
                return Err(e)
                    .chain_err(|| format!("Failed to create archive directory {:?}", archive.path));
            }
        }
        let header = ArchiveHeader { conserve_archive_version: String::from(ARCHIVE_VERSION) };
        let header_filename = path.join(HEADER_FILENAME);
        try!(jsonio::write(&header_filename, &header, &report)
            .chain_err(|| format!("Failed to write archive header: {:?}", header_filename)));
        info!("Created new archive in {:?}", path.display());
        Ok(archive)
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open(path: &Path, report: &Report) -> Result<Archive> {
        let header_path = path.join(HEADER_FILENAME);
        if !file_exists(&header_path)? {
            return Err(ErrorKind::NotAnArchive(path.into()).into());
        }
        let header: ArchiveHeader = jsonio::read(&header_path, &report)
            .chain_err(|| format!("Failed to read archive header"))?;
        if header.conserve_archive_version != ARCHIVE_VERSION {
            return Err(ErrorKind::UnsupportedArchiveVersion(header.conserve_archive_version)
                .into());
        }
        Ok(Archive { path: path.to_path_buf() })
    }

    /// Returns a iterator of ids for bands currently present, in arbitrary order.
    pub fn iter_bands_unsorted(self: &Archive) -> Result<IterBands> {
        let read_dir = try!(read_dir(&self.path)
            .chain_err(|| format!("failed reading directory {:?}", &self.path)));
        Ok(IterBands { dir_iter: read_dir })
    }

    /// Returns a vector of band ids, in sorted order.
    pub fn list_bands(self: &Archive) -> Result<Vec<BandId>> {
        let mut band_ids: Vec<BandId> = try!(try!(self.iter_bands_unsorted()).collect());
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

    // Return the `BandId` of the highest-numbered band, or ArchiveEmpty,
    // or an Err if any occurred reading the directory.
    pub fn last_band_id(self: &Archive) -> Result<BandId> {
        // Walk through list of bands; if any error return that, otherwise return the greatest.
        let mut accum: Option<BandId> = None;
        for next in try!(self.iter_bands_unsorted()) {
            accum = Some(match (next, accum) {
                (Err(e), _) => return Err(e),
                (Ok(b), None) => b,
                (Ok(b), Some(a)) => std::cmp::max(b, a),
            })
        }
        accum.ok_or(ErrorKind::ArchiveEmpty.into())
    }

    /// Make a new band. Bands are numbered sequentially.
    pub fn create_band(self: &Archive, report: &Report) -> Result<Band> {
        let new_band_id = match self.last_band_id() {
            Err(Error(ErrorKind::ArchiveEmpty, _)) => BandId::zero(),
            Ok(b) => b.next_sibling(),
            Err(e) => return Err(e),
        };
        Band::create(self.path(), new_band_id, report)
    }

    /// Open a specific named band.
    pub fn open_band(&self, band_id: &BandId, report: &Report) -> Result<Band> {
        Band::open(self.path(), band_id, report)
    }

    /// Open a band if specified, or the last.
    pub fn open_band_or_last(&self, band_id: &Option<BandId>, report: &Report) -> Result<Band> {
        let band_id = match band_id {
            &Some(ref b) => b.clone(),
            &None => try!(self.last_band_id()),
        };
        self.open_band(&band_id, report)
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
                }
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
                if let Ok(band_id) = BandId::from_string(&name_string) {
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
    use errors::ErrorKind;
    use {BandId, Report};
    use io::list_dir;
    use testfixtures::ScratchArchive;

    #[test]
    fn create_then_open_archive() {
        let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
        let arch_path = &testdir.path().join("arch");
        let arch = Archive::init(arch_path).unwrap();

        assert_eq!(arch.path(), arch_path.as_path());
        assert!(arch.list_bands().unwrap().is_empty());

        // We can re-open it.
        Archive::open(arch_path, &Report::new()).unwrap();
        assert!(arch.list_bands().unwrap().is_empty());
    }

    #[test]
    fn init_empty_dir() {
        let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
        let arch_path = testdir.path();
        let arch = Archive::init(arch_path).unwrap();

        assert_eq!(arch.path(), arch_path);
        assert!(arch.list_bands().unwrap().is_empty());

        Archive::open(arch_path, &Report::new()).unwrap();
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
        assert_eq!(contents, "{\"conserve_archive_version\":\"0.4\"}\n");
    }

    /// Can create bands in an archive.
    #[test]
    fn create_bands() {
        use super::super::io::directory_exists;
        let af = ScratchArchive::new();

        match *af.last_band_id().unwrap_err().kind() {
            ErrorKind::ArchiveEmpty => (),
            ref x => panic!("Unexpected error {:?}", x),
        }

        // Make one band
        let _band1 = af.create_band(&Report::new()).unwrap();
        assert!(directory_exists(af.path()).unwrap());
        let (_file_names, dir_names) = list_dir(af.path()).unwrap();
        println!("dirs: {:?}", dir_names);
        assert!(dir_names.contains("b0000"));

        assert_eq!(af.list_bands().unwrap(), vec![BandId::new(&[0])]);
        assert_eq!(af.last_band_id().unwrap(), BandId::new(&[0]));

        // Try creating a second band.
        let _band2 = &af.create_band(&Report::new()).unwrap();
        assert_eq!(af.list_bands().unwrap(),
                   vec![BandId::new(&[0]), BandId::new(&[1])]);
        assert_eq!(af.last_band_id().unwrap(), BandId::new(&[1]));
    }
}
