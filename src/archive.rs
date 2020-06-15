// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Archives holding backup material.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::errors::Error;
use crate::jsonio::{read_json, write_json};
use crate::kind::Kind;
use crate::misc::remove_item;
use crate::stats::ValidateArchiveStats;
use crate::transport::local::LocalTransport;
use crate::transport::{DirEntry, Transport};
use crate::*;

const HEADER_FILENAME: &str = "CONSERVE";
static BLOCK_DIR: &str = "d";

/// An archive holding backup material.
#[derive(Clone, Debug)]
pub struct Archive {
    /// Holds body content for all file versions.
    block_dir: BlockDir,

    transport: Box<dyn Transport>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ArchiveHeader {
    conserve_archive_version: String,
}

impl Archive {
    /// Make a new archive in a local direcotry.
    pub fn create_path(path: &Path) -> Result<Archive> {
        Archive::create(Box::new(LocalTransport::new(path)))
    }

    /// Make a new archive in a new directory accessed by a Transport.
    pub fn create(transport: Box<dyn Transport>) -> Result<Archive> {
        transport
            .create_dir("")
            .map_err(|source| Error::CreateArchiveDirectory { source })?;
        let names = transport.list_dir_names("").map_err(Error::from)?;
        if !names.files.is_empty() || !names.dirs.is_empty() {
            return Err(Error::NewArchiveDirectoryNotEmpty);
        }
        let block_dir = BlockDir::create(transport.sub_transport(BLOCK_DIR))?;
        write_json(
            &transport,
            HEADER_FILENAME,
            &ArchiveHeader {
                conserve_archive_version: String::from(ARCHIVE_VERSION),
            },
        )?;
        Ok(Archive {
            block_dir,
            transport,
        })
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open_path(path: &Path) -> Result<Archive> {
        Archive::open(Box::new(LocalTransport::new(path)))
    }

    pub fn open(transport: Box<dyn Transport>) -> Result<Archive> {
        let header: ArchiveHeader =
            read_json(&transport, HEADER_FILENAME).map_err(|err| match err {
                Error::IOError { source } => match source.kind() {
                    ErrorKind::NotFound => Error::NotAnArchive {},
                    _ => Error::ReadMetadata {
                        path: HEADER_FILENAME.to_owned(),
                        source,
                    },
                },
                other => other,
            })?;
        if header.conserve_archive_version != ARCHIVE_VERSION {
            return Err(Error::UnsupportedArchiveVersion {
                version: header.conserve_archive_version,
            });
        }
        let block_dir = BlockDir::open(transport.sub_transport(BLOCK_DIR));
        Ok(Archive {
            block_dir,
            transport,
        })
    }

    pub fn block_dir(&self) -> &BlockDir {
        &self.block_dir
    }

    /// Returns a vector of band ids, in sorted order from first to last.
    pub fn list_band_ids(&self) -> Result<Vec<BandId>> {
        let mut band_ids: Vec<BandId> = self.iter_band_ids_unsorted()?.collect();
        band_ids.sort_unstable();
        Ok(band_ids)
    }

    pub(crate) fn transport(&self) -> &dyn Transport {
        self.transport.as_ref()
    }

    /// Return an iterator of valid band ids in this archive, in arbitrary order.
    ///
    /// Errors reading the archive directory are logged and discarded.
    fn iter_band_ids_unsorted(&self) -> Result<impl Iterator<Item = BandId>> {
        // TODO: Count errors and return stats?
        Ok(self
            .transport
            .iter_dir_entries("")
            .map_err(|source| Error::ListBands { source })?
            .filter_map(|entry_r| match entry_r {
                // TODO: Count errors into stats.
                Err(e) => {
                    ui::problem(&format!("Error listing bands: {}", e));
                    None
                }
                Ok(DirEntry { name, kind, .. }) => {
                    if kind == Kind::Dir && name != BLOCK_DIR {
                        if let Ok(band_id) = name.parse() {
                            Some(band_id)
                        } else {
                            ui::problem(&format!("Unexpected directory {:?} in archive", name));
                            None
                        }
                    } else {
                        None
                    }
                }
            }))
    }

    /// Return the `BandId` of the highest-numbered band, or Ok(None) if there
    /// are no bands, or an Err if any occurred reading the directory.
    pub fn last_band_id(&self) -> Result<Option<BandId>> {
        Ok(self.iter_band_ids_unsorted()?.max())
    }

    /// Return the last completely-written band id, if any.
    pub fn last_complete_band(&self) -> Result<Option<Band>> {
        for id in self.list_band_ids()?.iter().rev() {
            let b = Band::open(self, &id)?;
            if b.is_closed()? {
                return Ok(Some(b));
            }
        }
        Ok(None)
    }

    /// Return a sorted set containing all the blocks referenced by all bands.
    pub fn referenced_blocks(&self) -> Result<BTreeSet<String>> {
        let mut hs = BTreeSet::<String>::new();
        for band_id in self.list_band_ids()? {
            let band = Band::open(&self, &band_id)?;
            for ie in band.iter_entries()? {
                for a in ie.addrs {
                    hs.insert(a.hash);
                }
            }
        }
        Ok(hs)
    }

    pub fn validate(&self) -> Result<ValidateArchiveStats> {
        let mut stats = self.validate_archive_dir()?;
        ui::println("Check blockdir...");
        stats.block_dir += self.block_dir.validate()?;
        self.validate_bands(&mut stats)?;

        if stats.has_problems() {
            ui::problem("Archive has some problems.");
        } else {
            ui::println("Archive is OK.");
        }
        Ok(stats)
    }

    fn validate_archive_dir(&self) -> Result<ValidateArchiveStats> {
        // TODO: Tests for the problems detected here.
        let mut stats = ValidateArchiveStats::default();
        ui::println("Check archive top-level directory...");

        let mut files: Vec<String> = Vec::new();
        let mut dirs: Vec<String> = Vec::new();
        for entry_result in self
            .transport
            .iter_dir_entries("")
            .map_err(|source| Error::ListBands { source })?
        {
            match entry_result {
                Ok(DirEntry { name, kind, .. }) => match kind {
                    Kind::Dir => dirs.push(name),
                    Kind::File => files.push(name),
                    other_kind => {
                        ui::problem(&format!(
                            "Unexpected file kind in archive directory: {:?} of kind {:?}",
                            name, other_kind
                        ));
                        stats.structure_problems += 1;
                    }
                },
                Err(source) => {
                    ui::problem(&format!("Error listing archive directory: {:?}", source));
                    stats.io_errors += 1;
                }
            }
        }
        remove_item(&mut files, &HEADER_FILENAME);
        if !files.is_empty() {
            stats.structure_problems += 1;
            ui::problem(&format!(
                "Unexpected files in archive directory {:?}: {:?}",
                self.transport, files
            ));
        }
        remove_item(&mut dirs, &BLOCK_DIR);
        dirs.sort();
        let mut bs = BTreeSet::<BandId>::new();
        for d in dirs.iter() {
            if let Ok(b) = d.parse() {
                if bs.contains(&b) {
                    stats.structure_problems += 1;
                    ui::problem(&format!(
                        "Duplicated band directory in {:?}: {:?}",
                        self.transport, d
                    ));
                } else {
                    bs.insert(b);
                }
            } else {
                stats.structure_problems += 1;
                ui::problem(&format!(
                    "Unexpected directory in {:?}: {:?}",
                    self.transport, d
                ));
            }
        }
        Ok(stats)
    }

    fn validate_bands(&self, _stats: &mut ValidateArchiveStats) -> Result<()> {
        // TODO: Don't stop early on any errors in the steps below, but do count them.
        // TODO: Better progress bars, that don't work by size but rather by
        // count.
        // TODO: Take in a dict of the known blocks and their decompressed lengths,
        // and use that to more cheaply check if the index is OK.
        ui::clear_bytes_total();
        for bid in self.list_band_ids()?.iter() {
            ui::println(&format!("Check {}...", bid));
            let b = Band::open(self, bid)?;
            b.validate()?;

            let st = StoredTree::open_incomplete_version(self, bid)?;
            st.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;

    use assert_fs::prelude::*;
    use assert_fs::TempDir;
    use spectral::prelude::*;

    use crate::test_fixtures::ScratchArchive;

    use super::*;

    #[test]
    fn create_then_open_archive() {
        let testdir = TempDir::new().unwrap();
        let arch_path = testdir.path().join("arch");
        let arch = Archive::create_path(&arch_path).unwrap();

        assert!(arch.list_band_ids().unwrap().is_empty());

        // We can re-open it.
        Archive::open_path(&arch_path).unwrap();
        assert!(arch.list_band_ids().unwrap().is_empty());
        assert!(arch.last_complete_band().unwrap().is_none());
    }

    #[test]
    fn fails_on_non_empty_directory() {
        let temp = TempDir::new().unwrap();

        temp.child("i am already here").touch().unwrap();

        let result = Archive::create_path(&temp.path());
        assert!(result.is_err());
        if let Err(Error::NewArchiveDirectoryNotEmpty) = result {
        } else {
            panic!("expected an error for a non-empty new archive directory")
        }

        temp.close().unwrap();
    }

    /// A new archive contains just one header file.
    /// The header is readable json containing only a version number.
    #[test]
    fn empty_archive() {
        let af = ScratchArchive::new();

        assert_that(&af.path()).is_a_directory();
        assert_that(&af.path().join("CONSERVE")).is_a_file();
        assert_that(&af.path().join("d")).is_a_directory();

        let header_path = af.path().join("CONSERVE");
        let mut header_file = fs::File::open(&header_path).unwrap();
        let mut contents = String::new();
        header_file.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "{\"conserve_archive_version\":\"0.6\"}\n");

        assert!(
            af.last_band_id().unwrap().is_none(),
            "Archive should have no bands yet"
        );
        assert!(
            af.last_complete_band().unwrap().is_none(),
            "Archive should have no bands yet"
        );
        assert!(af.referenced_blocks().unwrap().is_empty());
        assert_eq!(af.block_dir.block_names().unwrap().count(), 0);
    }

    #[test]
    fn create_bands() {
        let af = ScratchArchive::new();
        assert_that(&af.path().join("d")).is_a_directory();

        // Make one band
        let _band1 = Band::create(&af).unwrap();
        let band_path = af.path().join("b0000");
        assert_that(&band_path).is_a_directory();
        assert_that(&band_path.join("BANDHEAD")).is_a_file();
        assert_that(&band_path.join("i")).is_a_directory();

        assert_eq!(af.list_band_ids().unwrap(), vec![BandId::new(&[0])]);
        assert_eq!(af.last_band_id().unwrap(), Some(BandId::new(&[0])));

        // Try creating a second band.
        let _band2 = Band::create(&af).unwrap();
        assert_eq!(
            af.list_band_ids().unwrap(),
            vec![BandId::new(&[0]), BandId::new(&[1])]
        );
        assert_eq!(af.last_band_id().unwrap(), Some(BandId::new(&[1])));

        assert!(af.referenced_blocks().unwrap().is_empty());
        assert_eq!(af.block_dir.block_names().unwrap().count(), 0);
    }
}
