// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Bands are the top-level structure inside an archive.
//!
//! Each band contains up to one version of each file, arranged in sorted order within the
//! band.
//!
//! Bands can stack on top of each other to create a tree of incremental backups.


use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::block::BlockDir;
use super::index::IndexBuilder;
use super::io::directory_exists;

static BLOCK_DIR: &'static str = "d";
static INDEX_DIR: &'static str = "i";

/// Identifier for a band within an archive, eg 'b0001' or 'b0001-0020'.
///
/// `BandId`s implement a total ordering `std::cmp::Ord`.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord)]
pub struct BandId {
    /// The sequence numbers at each tier.
    seqs: Vec<u32>,

    /// The pre-calculated string form for this id.
    string_form: String,
}

// TODO: Maybe a more concise debug form?


impl BandId {
    /// Makes a new BandId from a sequence of integers.
    pub fn new(seqs: &[u32]) -> BandId {
        assert!(seqs.len() > 0);
        BandId {
            seqs: seqs.to_vec(),
            string_form: BandId::make_string_form(seqs),
        }
    }

    /// Return the origin BandId.
    pub fn zero() -> BandId {
        BandId::new(&[0])
    }

    /// Return the next BandId at the same level as self.
    pub fn next_sibling(self: &BandId) -> BandId {
        let mut next_seqs = self.seqs.clone();
        next_seqs[self.seqs.len() - 1] += 1;
        BandId::new(&next_seqs)
    }

    /// Make a new BandId from a string form.
    pub fn from_string(s: &str) -> Option<BandId> {
        if !s.starts_with('b') {
            return None;
        }
        let mut seqs = Vec::<u32>::new();
        for num_part in s[1..].split('-') {
            match num_part.parse::<u32>() {
                Ok(num) => seqs.push(num),
                Err(..) => return None,
            }
        }
        if seqs.is_empty() {
            None
        } else {
            // This rebuilds a new string form to get it into the canonical form.
            Some(BandId::new(&seqs))
        }
    }

    /// Returns the string representation of this BandId.
    ///
    /// Bands have an id which is a sequence of one or more non-negative integers.
    /// This is externally represented as a string like `b0001-0010`, which becomes
    /// their directory name in the archive.
    ///
    /// Numbers are zero-padded to what should normally be a reasonable length, but they can
    /// be longer.
    pub fn as_string(self: &BandId) -> &String {
        &self.string_form
    }

    fn make_string_form(seqs: &[u32]) -> String {
        let mut result = String::with_capacity(30);
        result.push_str("b");
        for s in seqs {
            result.push_str(&format!("{:04}-", s));
        }
        result.pop(); // remove the last dash
        result.shrink_to_fit();
        result
    }
}


/// All backup data is stored in a band.
#[derive(Debug)]
pub struct Band {
    id: BandId,
    path_buf: PathBuf,
    block_dir_path: PathBuf,
    index_dir_path: PathBuf,
}


impl Band {
    /// Make a new band (and its on-disk directory).
    ///
    /// Publicly, prefer Archive::create_band.
    pub fn create(in_directory: &Path, id: BandId) -> io::Result<Band> {
        let mut path_buf = in_directory.to_path_buf();
        path_buf.push(id.as_string());
        if try!(directory_exists(&path_buf)) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "band directory exists"));
        }

        let mut block_dir_path = path_buf.clone();
        block_dir_path.push(BLOCK_DIR);

        let mut index_dir_path = path_buf.clone();
        index_dir_path.push(INDEX_DIR);

        try!(fs::create_dir(path_buf.as_path()));
        try!(fs::create_dir(&block_dir_path));
        try!(fs::create_dir(&index_dir_path));
        info!("create band {:?}", path_buf);
        Ok(Band {
            id: id,
            path_buf: path_buf,
            block_dir_path: block_dir_path,
            index_dir_path: index_dir_path,
        })
    }

    pub fn path(self: &Band) -> &Path {
        &self.path_buf
    }

    pub fn block_dir(self: &Band) -> BlockDir {
        BlockDir::new(&self.block_dir_path)
    }

    pub fn index_builder(self: &Band) -> IndexBuilder {
        IndexBuilder::new(&self.index_dir_path)
    }
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use std::fs;
    use std::io;

    use super::*;
    use super::super::archive::scratch_archive;

    #[test]
    #[should_panic]
    fn empty_id_not_allowed() {
        BandId::new(&[]);
    }

    #[test]
    fn equality() {
        assert_eq!(BandId::new(&[1]), BandId::new(&[1]))
    }

    #[test]
    fn zero() {
        assert_eq!(BandId::zero().as_string(), "b0000");
    }

    #[test]
    fn next() {
        assert_eq!(BandId::zero().next_sibling().as_string(), "b0001");
        assert_eq!(BandId::new(&[2, 3]).next_sibling().as_string(),
                   "b0002-0004");
    }

    #[test]
    fn band_id_as_string() {
        let band_id = BandId::new(&[1, 10, 20]);
        assert_eq!(band_id.as_string(), "b0001-0010-0020");
        assert_eq!(BandId::new(&[1000000, 2000000]).as_string(),
                   "b1000000-2000000")
    }

    #[test]
    fn from_string_detects_invalid() {
        assert_eq!(BandId::from_string(""), None);
        assert_eq!(BandId::from_string("hello"), None);
        assert_eq!(BandId::from_string("b"), None);
        assert_eq!(BandId::from_string("b-"), None);
        assert_eq!(BandId::from_string("b2-"), None);
        assert_eq!(BandId::from_string("b-2"), None);
        assert_eq!(BandId::from_string("b2-1-"), None);
        assert_eq!(BandId::from_string("b2--1"), None);
        assert_eq!(BandId::from_string("beta"), None);
        assert_eq!(BandId::from_string("b-eta"), None);
        assert_eq!(BandId::from_string("b-1eta"), None);
        assert_eq!(BandId::from_string("b-1-eta"), None);
    }

    #[test]
    fn from_string_valid() {
        assert_eq!(BandId::from_string("b0001").unwrap().as_string(), "b0001");
        assert_eq!(BandId::from_string("b123456").unwrap().as_string(),
                   "b123456");
        assert_eq!(BandId::from_string("b0001-0100-0234").unwrap().as_string(),
                   "b0001-0100-0234");
    }

    #[test]
    fn create_band() {
        use super::super::io::list_dir;
        let (_tmpdir, archive) = scratch_archive();
        let band = Band::create(&archive.path(), BandId::from_string("b0001").unwrap()).unwrap();
        assert!(band.path().to_str().unwrap().ends_with("b0001"));
        assert!(fs::metadata(band.path()).unwrap().is_dir());

        let (file_names, dir_names) = list_dir(band.path()).unwrap();
        assert_eq!(file_names.len(), 0);
        assert_eq!(dir_names.len(), 2);
        assert!(dir_names.contains("d") && dir_names.contains("i"));
    }

    #[test]
    fn create_existing_band() {
        let (_tmpdir, archive) = scratch_archive();
        let band_id = BandId::from_string("b0001").unwrap();
        Band::create(&archive.path(), band_id.clone()).unwrap();
        match Band::create(&archive.path(), band_id) {
            Ok(_) => panic!("expected an error from existing band"),
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
            }
        }
    }
}
