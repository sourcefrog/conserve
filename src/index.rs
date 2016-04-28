// Conserve backup system.
// Copyright 2015 Martin Pool.

///! Listing of files in a band in the archive.

// use rustc_serialize::json;

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use super::apath::{apath_cmp, apath_valid};

/// Kind of file that can be stored in the archive.
#[derive(Debug, RustcDecodable, RustcEncodable)]
pub enum IndexKind {
    File,
    Dir,
    Symlink,
}

/// Description of one archived file.
#[derive(Debug, RustcDecodable, RustcEncodable)]
pub struct IndexEntry {
    /// Path of this entry relative to the base of the backup, in `apath` form.
    pub apath: String,

    /// File modification time, in whole seconds past the Unix epoch.
    pub mtime: u64,

    /// Type of file.
    pub kind: IndexKind,

    /// BLAKE2b hash of the entire original file.
    pub blake2b: String,
}


/// Accumulates ordered changes to the index and streams them out to index files.
pub struct IndexBuilder {
    dir: PathBuf,
    entries: Vec<IndexEntry>,
}


/// Accumulate and write out index entries into files in an index directory.
impl IndexBuilder {
    /// Make a new builder that will write files into the given directory.
    pub fn new(dir: &Path) -> IndexBuilder {
        IndexBuilder {
            dir: dir.to_path_buf(),
            entries: Vec::<IndexEntry>::new(),
        }
    }

    pub fn push(&mut self, entry: IndexEntry) {
        // We do this check here rather than the Index constructor so that we
        // can still read invalid apaths...
        if !apath_valid(&entry.apath) {
            panic!("invalid apath: {:?}", &entry.apath);
        }
        if !self.entries.is_empty() {
            let last_apath = &self.entries.last().unwrap().apath;
            assert_eq!(apath_cmp(last_apath, &entry.apath), Ordering::Less);
        }

        self.entries.push(entry);
    }
}


#[cfg(test)]
mod tests {
    use rustc_serialize::json;
    use tempdir;

    use super::{IndexBuilder, IndexEntry, IndexKind};

    const EXAMPLE_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf21\
         45b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    #[test]
    fn test_serialize_index() {
        let entries = [IndexEntry {
            apath: "a/b".to_string(),
            mtime: 1461736377,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        }];
        let index_json = json::encode(&entries).unwrap();
        println!("{}", index_json);
        assert_eq!(
            index_json,
            "[{\"apath\":\"a/b\",\
            \"mtime\":1461736377,\
            \"kind\":\"File\",\
            \"blake2b\":\"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117\
            b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c\"}]");
    }

    #[test]
    #[should_panic]
    fn test_index_builder_checks_order() {
        let testdir = tempdir::TempDir::new("index_test").unwrap();
        let mut ib = IndexBuilder::new(testdir.path());
        ib.push(IndexEntry {
            apath: "zzz".to_string(),
            mtime: 0,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        });
        ib.push(IndexEntry {
            apath: "aaa".to_string(),
            mtime: 0,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        });
    }

    #[test]
    #[should_panic]
    fn test_index_builder_checks_names() {
        let testdir = tempdir::TempDir::new("index_test").unwrap();
        let mut ib = IndexBuilder::new(testdir.path());
        ib.push(IndexEntry {
            apath: "/dev/null".to_string(),
            mtime: 0,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        })
    }
}
