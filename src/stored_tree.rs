// Copyright 2017 Martin Pool.

//! Access a versioned tree stored in the archive.
//!
//! Through this interface you can iterate the contents and retrieve file contents.
//!
//! This is the preferred higher-level interface for reading stored versions. It'll abstract
//! across incremental backups, hiding from the caller that data may be distributed across
//! multiple index files, bands, and blocks.

use globset::GlobSet;

use super::*;


/// Read index and file contents for a version stored in the archive.
///
/// These are obtained from `Archive.stored_tree`.
#[derive(Debug)]
pub struct StoredTree {
    archive: Archive,
    band: Band,
}


impl StoredTree {
    pub(super) fn open(archive: &Archive, band_id: &Option<BandId>) -> Result<StoredTree> {
        let band = archive.open_band(band_id)?;
        // TODO: Maybe warn if the band's incomplete, or fail unless opening is forced?
        Ok(StoredTree {
            archive: archive.clone(),
            band: band,
        })
    }

    pub fn band(&self) -> &Band {
        &self.band
    }

    pub fn is_closed(&self) -> Result<bool> {
        self.band.is_closed()
    }

    /// Return an iter of index entries in this stored tree.
    pub fn index_iter(&self, excludes: &GlobSet) -> Result<index::Iter> {
        self.band.index_iter(excludes, self.archive.report())
    }

    /// Return an iter of contents of file contents for the given file entry.
    ///
    /// Contents are yielded as blocks of bytes, of arbitrary length as stored in the archive.
    pub fn file_contents(
        &self,
        entry: &index::Entry,
    ) -> Result<stored_file::StoredFile> {
        Ok(stored_file::StoredFile::open(
            self.band.block_dir(),
            entry.addrs.clone(),
            self.archive.report(),
        ))
    }

    // TODO: Perhaps add a way to open a file by name, bearing in mind this might be slow to
    // call repeatedly if it reads the whole index.
}


#[cfg(test)]
mod test {
    use super::super::*;
    use super::super::test_fixtures::*;

    #[test]
    pub fn open_stored_tree() {
        let af = ScratchArchive::new();
        af.store_two_versions();

        let last_band_id = af.last_band_id().unwrap();
        let st = af.stored_tree(&None).unwrap();

        assert_eq!(st.band().id(), last_band_id);

        let names: Vec<String> = st.index_iter(&excludes::excludes_nothing())
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
        assert!(af.stored_tree(&None).is_err());
    }
}
