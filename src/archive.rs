// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Archives holding backup material.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::Error;
use crate::jsonio;
use crate::kind::Kind;
use crate::misc::remove_item;
use crate::stats::ValidateArchiveStats;
use crate::transport::local::LocalTransport;
use crate::transport::TransportRead;
use crate::*;

const HEADER_FILENAME: &str = "CONSERVE";
static BLOCK_DIR: &str = "d";

/// An archive holding backup material.
#[derive(Clone)]
pub struct Archive {
    /// Top-level directory for the archive.
    path: PathBuf,

    /// Holds body content for all file versions.
    block_dir: BlockDir,

    transport: LocalTransport,
}

#[derive(Debug, Serialize, Deserialize)]
struct ArchiveHeader {
    conserve_archive_version: String,
}

impl Archive {
    /// Make a new directory to hold an archive, and write the header.
    pub fn create(path: &Path) -> Result<Archive> {
        std::fs::create_dir(&path).map_err(|source| Error::CreateArchiveDirectory {
            path: path.to_owned(),
            source,
        })?;
        let block_dir = BlockDir::create(&path.join(BLOCK_DIR))?;
        let header = ArchiveHeader {
            conserve_archive_version: String::from(ARCHIVE_VERSION),
        };
        jsonio::write_json_metadata_file(&path.join(HEADER_FILENAME), &header)?;
        Ok(Archive {
            path: path.to_owned(),
            block_dir,
            transport: LocalTransport::new(path),
        })
    }

    /// Open an existing archive.
    ///
    /// Checks that the header is correct.
    pub fn open<P: Into<PathBuf>>(path: P) -> Result<Archive> {
        let path: PathBuf = path.into();
        let mut transport = LocalTransport::new(&path);
        let header_json = transport.read_file(HEADER_FILENAME).map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                Error::NotAnArchive { path: path.clone() }
            } else {
                Error::ReadMetadata {
                    path: path.clone(),
                    source: e,
                }
            }
        })?;
        let header: ArchiveHeader =
            serde_json::from_slice(&header_json).map_err(|source| Error::DeserializeJson {
                source,
                path: path.clone(),
            })?;
        if header.conserve_archive_version != ARCHIVE_VERSION {
            return Err(Error::UnsupportedArchiveVersion {
                version: header.conserve_archive_version,
                path: path.clone(),
            });
        }
        let block_dir = BlockDir::new(&path.join(BLOCK_DIR));
        Ok(Archive {
            path,
            block_dir,
            transport,
        })
    }

    pub fn block_dir(&self) -> &BlockDir {
        &self.block_dir
    }

    /// Returns the top-level directory for the archive.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Returns a vector of band ids, in sorted order from first to last.
    pub fn list_bands(&self) -> Result<Vec<BandId>> {
        let mut band_ids: Vec<BandId> = self.iter_band_ids_unsorted()?.collect();
        band_ids.sort_unstable();
        Ok(band_ids)
    }

    /// Return an iterator of valid band ids in this archive, in arbitrary order.
    ///
    /// Errors reading the archive directory are logged and discarded.
    fn iter_band_ids_unsorted(&self) -> Result<impl Iterator<Item = BandId>> {
        // TODO: Count errors and return stats?
        Ok(self
            .transport
            .read_dir("")
            .map_err(|source| Error::ListBands {
                path: self.path.clone(),
                source,
            })?
            .filter_map(|entry_r| match entry_r {
                // TODO: Count errors into stats.
                Err(e) => {
                    ui::problem(&format!("Error listing bands: {}", e));
                    None
                }
                Ok(entry) => {
                    let name = entry.name_tail();
                    if entry.kind() == Kind::Dir && name != BLOCK_DIR {
                        if let Ok(band_id) = name.parse() {
                            Some(band_id)
                        } else {
                            ui::problem(&format!(
                                "Unexpected directory {:?} in archive",
                                entry.relpath()
                            ));
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
        for id in self.list_bands()?.iter().rev() {
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
        for band_id in self.list_bands()? {
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
            .read_dir("")
            .map_err(|source| Error::ReadMetadata {
                source,
                path: self.path.to_owned(),
            })?
        {
            match entry_result {
                Ok(entry) => match entry.kind() {
                    Kind::Dir => dirs.push(entry.name_tail().to_owned()),
                    Kind::File => files.push(entry.name_tail().to_owned()),
                    other_kind => {
                        ui::problem(&format!(
                            "Unexpected file kind in archive directory: {:?} of kind {:?}",
                            entry.relpath(),
                            other_kind
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
                self.path(),
                files
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
                        self.path(),
                        d
                    ));
                } else {
                    bs.insert(b);
                }
            } else {
                stats.structure_problems += 1;
                ui::problem(&format!(
                    "Unexpected directory in {:?}: {:?}",
                    self.path(),
                    d
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
        for bid in self.list_bands()?.iter() {
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
    use tempfile::TempDir;

    use super::*;
    use crate::test_fixtures::ScratchArchive;

    #[test]
    fn create_then_open_archive() {
        let testdir = TempDir::new().unwrap();
        let arch_path = testdir.path().join("arch");
        let arch = Archive::create(&arch_path).unwrap();

        assert_eq!(arch.path(), arch_path.as_path());
        assert!(arch.list_bands().unwrap().is_empty());

        // We can re-open it.
        Archive::open(arch_path).unwrap();
        assert!(arch.list_bands().unwrap().is_empty());
        assert!(arch.last_complete_band().unwrap().is_none());
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

        // Make one band
        let _band1 = Band::create(&af).unwrap();
        let (_file_names, dir_names) = list_dir(af.path()).unwrap();
        assert_eq!(dir_names, &["b0000", "d"]);

        assert_eq!(af.list_bands().unwrap(), vec![BandId::new(&[0])]);
        assert_eq!(af.last_band_id().unwrap(), Some(BandId::new(&[0])));

        // Try creating a second band.
        let _band2 = Band::create(&af).unwrap();
        assert_eq!(
            af.list_bands().unwrap(),
            vec![BandId::new(&[0]), BandId::new(&[1])]
        );
        assert_eq!(af.last_band_id().unwrap(), Some(BandId::new(&[1])));

        assert!(af.referenced_blocks().unwrap().is_empty());
        assert_eq!(af.block_dir.block_names().unwrap().count(), 0);
    }
}
