// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Bands are the top-level structure inside an archive.
//!
//! Each band contains up to one version of each file, arranged in sorted order within the
//! band.
//!
//! Bands can stack on top of each other to create a tree of incremental backups.

/// Bands have an id which is a sequence of one or more non-negative integers. This is externally
/// represented as a string like `b0001-0010`, which becomes their directory name in the archive.
///
/// ```
/// use conserve::band::BandId;
/// let band_id = BandId::new(&[1, 10, 20]).unwrap();
/// assert_eq!(band_id.as_string(), "b0001-0010-0020");
/// ```
///
/// Numbers are zero-padded to what should normally be a reasonable length, but they can
/// overflow:
///
/// ```
/// use conserve::band::BandId;
/// let band_id = BandId::new(&[1000000, 2000000]).unwrap();
/// assert_eq!(band_id.as_string(),
///            "b1000000-2000000")
/// ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::io::directory_exists;

static BLOCK_DIR: &'static str = "d";
static INDEX_DIR: &'static str = "i";

#[derive(Debug, PartialEq, Clone)]
pub struct BandId {
    /// The sequence numbers at each tier.
    seqs: Vec<u32>,

    /// The pre-calculated string form for this id.
    string_form: String,
}


impl BandId {
    /// Makes a new BandId from a sequence of integers.
    pub fn new(seqs: &[u32]) -> Option<BandId> {
        if seqs.len() > 0 {
            Some(BandId{
                seqs: seqs.to_vec(),
                string_form: BandId::make_string_form(seqs),
            })
        } else {
            None
        }
    }

    /// Make a new BandId from a string form.
    ///
    /// ```
    /// use conserve::band::BandId;
    /// let band = BandId::from_string("b0001-1234").unwrap();
    /// assert_eq!(band.as_string(), "b0001-1234");
    /// ```
    pub fn from_string(s: &str) -> Option<BandId> {
        if s.chars().next() != Some('b') {
            return None
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
            BandId::new(&seqs)
        }
    }

    /// Returns the string representation of this BandId.
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

    // TODO: Maybe a more concise debug form?
}


/// All backup data is stored in a band.
#[derive(Debug)]
pub struct Band {
    id: BandId,
    path_buf: PathBuf,
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
        try!(fs::create_dir(path_buf.as_path()));

        let mut subdir_path = path_buf.clone();
        subdir_path.push(BLOCK_DIR);
        try!(fs::create_dir(&subdir_path));
        subdir_path.set_file_name(INDEX_DIR);
        try!(fs::create_dir(&subdir_path));
        Ok(Band{
            id: id,
            path_buf: path_buf,
        })
    }

    pub fn path(self: &Band) -> &Path {
        &self.path_buf
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
    fn test_empty_id_not_allowed() {
        assert!(BandId::new(&[]).is_none());
    }

    #[test]
    fn test_from_string_detects_invalid() {
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
    fn test_from_string_valid() {
        assert_eq!(BandId::from_string("b0001").unwrap().as_string(), "b0001");
        assert_eq!(BandId::from_string("b123456").unwrap().as_string(), "b123456");
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
