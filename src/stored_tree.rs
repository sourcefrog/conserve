// Copyright 2017, 2018 Martin Pool.

//! Access a versioned tree stored in the archive.
//!
//! Through this interface you can iterate the contents and retrieve file contents.
//!
//! This is the preferred higher-level interface for reading stored versions. It'll abstract
//! across incremental backups, hiding from the caller that data may be distributed across
//! multiple index files, bands, and blocks.

use std::io::prelude::*;

use blake2_rfc::blake2b::Blake2b;
use rayon::prelude::*;
use rustc_serialize::hex::ToHex;

use super::blockdir::{BLAKE_HASH_SIZE_BYTES, MAX_BLOCK_SIZE};
use super::stored_file::StoredFile;
use super::*;

/// Read index and file contents for a version stored in the archive.
#[derive(Debug)]
pub struct StoredTree {
    archive: Archive,
    band: Band,
    excludes: GlobSet,
    index: ReadIndex,
}

impl StoredTree {
    fn new(archive: &Archive, band: Band, excludes: GlobSet) -> StoredTree {
        let index = band.index();
        StoredTree {
            archive: archive.clone(),
            band,
            excludes,
            index,
        }
    }

    /// Open the last complete version in the archive.
    pub fn open_last(archive: &Archive) -> Result<StoredTree> {
        Ok(StoredTree::new(
            archive,
            archive.last_complete_band()?,
            excludes::excludes_nothing(),
        ))
    }

    /// Open a specified version.
    ///
    /// It's an error if it's not complete.
    pub fn open_version(archive: &Archive, band_id: &BandId) -> Result<StoredTree> {
        let band = Band::open(archive, band_id)?;
        if !band.is_closed()? {
            return Err(Error::BandIncomplete(band_id.clone()));
        }
        Ok(StoredTree::new(archive, band, excludes::excludes_nothing()))
    }

    /// Open a specified version.
    ///
    /// This function allows opening incomplete versions, which might contain only a partial copy
    /// of the source tree, or maybe nothing at all.
    pub fn open_incomplete_version(archive: &Archive, band_id: &BandId) -> Result<StoredTree> {
        let band = Band::open(archive, band_id)?;
        Ok(StoredTree::new(archive, band, excludes::excludes_nothing()))
    }

    pub fn with_excludes(self, excludes: GlobSet) -> StoredTree {
        StoredTree { excludes, ..self }
    }

    pub fn band(&self) -> &Band {
        &self.band
    }

    pub fn archive(&self) -> &Archive {
        &self.archive
    }

    pub fn is_closed(&self) -> Result<bool> {
        self.band.is_closed()
    }

    pub fn validate(&self) -> Result<()> {
        let report = self.report();
        report.set_phase(format!("Check tree {}", self.band().id()));
        // TODO: Maybe parallel over chunks; don't read the whole index into
        // memory?
        // TODO: Remove unwraps.
        let ev = self
            .iter_entries(self.report())?
            .map(|e| e.unwrap())
            .filter(|e| e.kind() == Kind::File)
            .collect::<Vec<_>>();
        ev.par_iter()
            .map(|e| {
                report.start_entry(e);
                let mut in_buf = vec![0; MAX_BLOCK_SIZE];
                let mut fc = self.file_contents(&e).unwrap();
                let mut file_hasher = Blake2b::new(BLAKE_HASH_SIZE_BYTES);
                loop {
                    let read_bytes = fc.read(&mut in_buf).unwrap();
                    if read_bytes == 0 {
                        break;
                    }
                    file_hasher.update(&in_buf[0..read_bytes]);
                }
                let file_hash = file_hasher.finalize().as_bytes().to_hex();
                assert_eq!(file_hash, e.blake2b().unwrap());
                report.increment_work(e.size().unwrap_or(0));
            })
            .count();
        Ok(())
    }

    // TODO: Perhaps add a way to open a file by name, bearing in mind this might be slow to
    // call if it reads the whole index.
}

impl ReadTree for StoredTree {
    type E = index::IndexEntry;
    type I = index::Iter;
    type R = stored_file::StoredFile;

    /// Return an iter of index entries in this stored tree.
    fn iter_entries(&self, report: &Report) -> Result<index::Iter> {
        self.band.index().iter(&self.excludes, report)
    }

    fn file_contents(&self, entry: &Self::E) -> Result<Self::R> {
        Ok(StoredFile::open(
            self.archive.block_dir().clone(),
            entry.addrs.clone(),
            self.report(),
        ))
    }

    fn estimate_count(&self) -> Result<u64> {
        self.index.estimate_entry_count()
    }
}

impl HasReport for StoredTree {
    fn report(&self) -> &Report {
        self.archive.report()
    }
}

#[cfg(test)]
mod test {
    use super::super::test_fixtures::*;
    use super::super::*;

    #[test]
    pub fn open_stored_tree() {
        let af = ScratchArchive::new();
        af.store_two_versions();

        let last_band_id = af.last_band_id().unwrap();
        let st = StoredTree::open_last(&af).unwrap();

        assert_eq!(st.band().id(), last_band_id);

        let names: Vec<String> = st
            .iter_entries(&af.report())
            .unwrap()
            .map(|e| e.unwrap().apath)
            .collect();
        let expected = if SYMLINKS_SUPPORTED {
            vec![
                "/",
                "/hello",
                "/hello2",
                "/link",
                "/subdir",
                "/subdir/subfile",
            ]
        } else {
            vec!["/", "/hello", "/hello2", "/subdir", "/subdir/subfile"]
        };
        assert_eq!(expected, names);
    }

    #[test]
    pub fn cant_open_no_versions() {
        let af = ScratchArchive::new();
        assert!(StoredTree::open_last(&af).is_err());
    }
}
