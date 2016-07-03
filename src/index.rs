// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

///! Listing of files in a band in the archive.

// use rustc_serialize::json;

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use rustc_serialize::json;

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

    pub fn to_json(&self) -> String {
        json::encode(&self.entries).unwrap()
    }

    /// Return the subdirectory for a hunk numbered `hunk_number`.
    pub fn subdir_for_hunk(&self, hunk_number: u32) -> PathBuf {
        let mut buf = self.dir.clone();
        buf.push(format!("{:05}", hunk_number / 10000));
        buf
    }

    /// Return the filename (in subdirectory) for a hunk.
    pub fn path_for_hunk(&self, hunk_number: u32) -> PathBuf {
        let mut buf = self.subdir_for_hunk(hunk_number);
        buf.push(format!("{:09}", hunk_number));
        buf
    }
}


#[cfg(test)]
mod tests {
    use rustc_serialize::json;
    use std::path::Path;
    use tempdir;

    use super::{IndexBuilder, IndexEntry, IndexKind};

    const EXAMPLE_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf21\
         45b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    fn scratch_indexbuilder() -> (tempdir::TempDir, IndexBuilder) {
        let testdir = tempdir::TempDir::new("index_test").unwrap();
        let ib = IndexBuilder::new(testdir.path());
        (testdir, ib)
    }

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
            r#"[{"apath":"a/b","mtime":1461736377,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c"}]"#);
    }

    #[test]
    #[should_panic]
    fn test_index_builder_checks_order() {
        let (_testdir, mut ib) = scratch_indexbuilder();
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
        let (_testdir, mut ib) = scratch_indexbuilder();
        ib.push(IndexEntry {
            apath: "/dev/null".to_string(),
            mtime: 0,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        })
    }

    #[test]
    fn test_index_to_json() {
        let (_testdir, mut ib) = scratch_indexbuilder();
        ib.push(IndexEntry {
            apath: "hello".to_string(),
            mtime: 0,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        });
        let json = ib.to_json();
        assert_eq!(json,
            r#"[{"apath":"hello","mtime":0,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c"}]"#)
    }

    #[test]
    fn test_path_for_hunk() {
        let index_dir = Path::new("/foo");
        let ib = IndexBuilder::new(index_dir);
        let hunk_path = ib.path_for_hunk(0);
        assert_eq!(hunk_path.file_name().unwrap().to_str().unwrap(),
            "000000000");
        assert_eq!(hunk_path.parent().unwrap().file_name().unwrap().to_str().unwrap(),
            "00000");
    }
}
