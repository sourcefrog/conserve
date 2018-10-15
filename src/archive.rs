// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Archives holding backup material.
//!
//! Archives must be initialized before use, which creates the directory.
//!
//! Archives can contain a tree of bands, which themselves contain file versions.

use std::collections::BTreeSet;
use std::fs;
use std::fs::read_dir;
use std::path::{Path, PathBuf};

use super::io::{file_exists, require_empty_directory};
use super::jsonio;
use super::*;

const HEADER_FILENAME: &str = "CONSERVE";
static BLOCK_DIR: &'static str = "d";

/// An archive holding backup material.
#[derive(Clone, Debug)]
pub struct Archive {
    /// Top-level directory for the archive.
    path: PathBuf,

    /// Report for operations on this archive.
    report: Report,

    /// Holds body content for all file versions.
    block_dir: BlockDir,
}

#[derive(Debug, RustcDecodable, RustcEncodable)]
struct ArchiveHeader {
    conserve_archive_version: String,
}

impl Archive {
    /// Make a new directory to hold an archive, and write the header.
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Archive> {
        let path = path.as_ref();
        require_empty_directory(path)?;
        let block_dir = BlockDir::create(&path.join(BLOCK_DIR))?;
        let header = ArchiveHeader {
            conserve_archive_version: String::from(ARCHIVE_VERSION),
        };
        let header_filename = path.join(HEADER_FILENAME);
        let report = Report::new();
        jsonio::write(&header_filename, &header, &report)
            .chain_err(|| format!("Failed to write archive header: {:?}", header_filename))?;
        Ok(Archive {
            path: path.to_path_buf(),
            report,
            block_dir,
        })
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open<P: AsRef<Path>>(path: P, report: &Report) -> Result<Archive> {
        let path = path.as_ref();
        let header_path = path.join(HEADER_FILENAME);
        if !file_exists(&header_path)? {
            return Err(ErrorKind::NotAnArchive(path.into()).into());
        }
        let block_dir = BlockDir::new(&path.join(BLOCK_DIR));
        let header: ArchiveHeader =
            jsonio::read(&header_path, &report).chain_err(|| "Failed to read archive header")?;
        if header.conserve_archive_version != ARCHIVE_VERSION {
            return Err(
                ErrorKind::UnsupportedArchiveVersion(header.conserve_archive_version).into(),
            );
        }
        Ok(Archive {
            path: path.to_path_buf(),
            report: report.clone(),
            block_dir,
        })
    }

    pub fn block_dir(&self) -> &BlockDir {
        &self.block_dir
    }

    /// Returns a iterator of ids for bands currently present, in arbitrary order.
    pub fn iter_bands_unsorted(&self) -> Result<IterBands> {
        let read_dir = read_dir(&self.path)
            .chain_err(|| format!("failed reading directory {:?}", &self.path))?;
        Ok(IterBands {
            dir_iter: read_dir,
            report: self.report.clone(),
        })
    }

    /// Returns a vector of band ids, in sorted order from first to last.
    pub fn list_bands(&self) -> Result<Vec<BandId>> {
        let mut band_ids = Vec::<BandId>::new();
        for b in self.iter_bands_unsorted()? {
            band_ids.push(b?);
        }
        band_ids.sort_unstable();
        Ok(band_ids)
    }

    /// Returns the top-level directory for the archive.
    ///
    /// The top-level directory contains a `CONSERVE` header file, and zero or more
    /// band directories.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Return the `BandId` of the highest-numbered band, or ArchiveEmpty,
    /// or an Err if any occurred reading the directory.
    pub fn last_band_id(&self) -> Result<BandId> {
        // Walk through list of bands; if any error return that, otherwise return the greatest.
        let mut accum: Option<BandId> = None;
        for next in self.iter_bands_unsorted()? {
            accum = Some(match (next, accum) {
                (Err(e), _) => return Err(e),
                (Ok(b), None) => b,
                (Ok(b), Some(a)) => std::cmp::max(b, a),
            })
        }
        accum.ok_or_else(|| ErrorKind::ArchiveEmpty.into())
    }

    /// Return the last completely-written band id.
    pub fn last_complete_band(&self) -> Result<Band> {
        for id in self.list_bands()?.iter().rev() {
            let b = Band::open(self, &id)?;
            if b.is_closed()? {
                return Ok(b);
            }
        }
        Err(ErrorKind::NoCompleteBands.into())
    }

    /// Return a sorted set containing all the blocks referenced by all bands.
    pub fn referenced_blocks(&self) -> Result<BTreeSet<String>> {
        let mut hs = BTreeSet::<String>::new();
        for band_id in self.iter_bands_unsorted()? {
            let band = Band::open(&self, &band_id?)?;
            for ie in band.index_iter(&excludes::excludes_nothing(), &self.report)? {
                for a in ie?.addrs {
                    hs.insert(a.hash);
                }
            }
        }
        Ok(hs)
    }
}

impl HasReport for Archive {
    /// Return the Report that counts operations on this Archive and objects descended from it.
    fn report(&self) -> &Report {
        &self.report
    }
}

pub struct IterBands {
    dir_iter: fs::ReadDir,
    report: Report,
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
                // TODO: Complain about non-directory children other than the expected header?
                continue;
            } else if let Ok(name_string) = entry.file_name().into_string() {
                if name_string == BLOCK_DIR {
                    continue;
                } else if let Ok(band_id) = BandId::from_string(&name_string) {
                    return Some(Ok(band_id));
                } else {
                    self.report.problem(&format!(
                        "unexpected archive subdirectory {:?}",
                        &name_string
                    ));
                }
            } else {
                self.report.problem(&format!(
                    "unexpected archive subdirectory with un-decodable name {:?}",
                    entry.file_name()
                ));
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
    use test_fixtures::ScratchArchive;

    #[test]
    fn create_then_open_archive() {
        let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
        let arch_path = &testdir.path().join("arch");
        let arch = Archive::create(arch_path).unwrap();

        assert_eq!(arch.path(), arch_path.as_path());
        assert!(arch.list_bands().unwrap().is_empty());

        // We can re-open it.
        Archive::open(arch_path, &Report::new()).unwrap();
        assert!(arch.list_bands().unwrap().is_empty());
        assert!(arch.last_complete_band().is_err());
    }

    #[test]
    fn init_empty_dir() {
        let testdir = tempdir::TempDir::new("conserve-tests").unwrap();
        let arch_path = testdir.path();
        let arch = Archive::create(arch_path).unwrap();

        assert_eq!(arch.path(), arch_path);
        assert!(arch.list_bands().unwrap().is_empty());

        Archive::open(arch_path, &Report::new()).unwrap();
        assert!(arch.list_bands().unwrap().is_empty());
    }

    /// A new archive contains just one header file.
    /// The header is readable json containing only a version number.
    #[test]
    fn empty_archive() {
        let af = ScratchArchive::new();
        let (file_names, dir_names) = list_dir(af.path()).unwrap();
        assert_eq!(file_names, &["CONSERVE"]);
        assert_eq!(dir_names, &["d"]);

        let header_path = af.path().join("CONSERVE");
        let mut header_file = fs::File::open(&header_path).unwrap();
        let mut contents = String::new();
        header_file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "{\"conserve_archive_version\":\"0.5\"}\n");

        match *af.last_band_id().unwrap_err().kind() {
            ErrorKind::ArchiveEmpty => (),
            ref x => panic!("Unexpected error {:?}", x),
        }

        match *af.last_complete_band().unwrap_err().kind() {
            ErrorKind::NoCompleteBands => (),
            ref x => panic!("Unexpected error {:?}", x),
        }

        assert!(af.referenced_blocks().unwrap().is_empty());
        assert!(af.block_dir.blocks(&af.report).unwrap().is_empty());
    }

    #[test]
    fn create_bands() {
        use super::super::io::directory_exists;
        let af = ScratchArchive::new();

        // Make one band
        let _band1 = Band::create(&af).unwrap();
        assert!(directory_exists(af.path()).unwrap());
        let (_file_names, dir_names) = list_dir(af.path()).unwrap();
        assert_eq!(dir_names, &["b0000", "d"]);

        assert_eq!(af.list_bands().unwrap(), vec![BandId::new(&[0])]);
        assert_eq!(af.last_band_id().unwrap(), BandId::new(&[0]));

        // Try creating a second band.
        let _band2 = Band::create(&af).unwrap();
        assert_eq!(
            af.list_bands().unwrap(),
            vec![BandId::new(&[0]), BandId::new(&[1])]
        );
        assert_eq!(af.last_band_id().unwrap(), BandId::new(&[1]));

        assert!(af.referenced_blocks().unwrap().is_empty());
        assert!(af.block_dir.blocks(&af.report).unwrap().is_empty());
    }
}
