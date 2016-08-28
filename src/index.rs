// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

///! Listing of files in a band in the archive.

use std::cmp::Ordering;
use std::io;
use std::path::{Path, PathBuf};

use rustc_serialize::json;

use super::apath;
use super::io::{write_compressed_bytes};
use super::report::Report;

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
#[derive(Debug)]
pub struct IndexBuilder {
    /// The `i` directory within the band where all files for this index are written.
    dir: PathBuf,

    /// Currently queued entries to be written out.
    entries: Vec<IndexEntry>,

    /// Index hunk number, starting at 0.
    sequence: u32,

    /// The last-added filename, to enforce ordering.  At the start of the first hunk
    /// this is empty; at the start of a later hunk it's the last path from the previous
    /// hunk, and otherwise it's the last path from `entries`.
    last_apath: Option<String>,
}


/// Accumulate and write out index entries into files in an index directory.
impl IndexBuilder {
    /// Make a new builder that will write files into the given directory.
    pub fn new(dir: &Path) -> IndexBuilder {
        IndexBuilder {
            dir: dir.to_path_buf(),
            entries: Vec::<IndexEntry>::new(),
            sequence: 0,
            last_apath: None,
        }
    }

    /// Append an entry to the index.
    ///
    /// The new entry must sort after everything already written to the index.
    pub fn push(&mut self, entry: IndexEntry) {
        // We do this check here rather than the Index constructor so that we
        // can still read invalid apaths...
        if !apath::valid(&entry.apath) {
            panic!("invalid apath: {:?}", &entry.apath);
        }
        if let Some(ref last_apath) = self.last_apath {
            assert_eq!(apath::cmp(&last_apath, &entry.apath), Ordering::Less);
        }
        self.last_apath = Some(entry.apath.clone());
        self.entries.push(entry);
    }

    pub fn to_json(&self) -> String {
        json::encode(&self.entries).unwrap()
    }

    /// Finish this hunk of the index.
    ///
    /// This writes all the currently queued entries into a new index file
    /// in the band directory, and then clears the index to start receiving
    /// entries for the next hunk.
    pub fn finish_hunk(&mut self, report: &mut Report) -> io::Result<()> {
        let json_str = self.to_json();
        let json_bytes = json_str.as_bytes();
        try!(super::io::ensure_dir_exists(
            &subdir_for_hunk(&self.dir, self.sequence)));
        let hunk_path = &path_for_hunk(&self.dir, self.sequence);
        let compressed_len = try!(write_compressed_bytes(hunk_path, json_bytes, report));

        report.increment_size("index.write", json_bytes.len() as u64, compressed_len as u64);
        report.increment("index.write.hunks", 1);

        // Ready for the next hunk.
        self.entries.clear();
        self.sequence += 1;
        Ok(())
    }
}


/// Return the subdirectory for a hunk numbered `hunk_number`.
fn subdir_for_hunk(dir: &Path, hunk_number: u32) -> PathBuf {
    let mut buf = dir.to_path_buf();
    buf.push(format!("{:05}", hunk_number / 10000));
    buf
}

/// Return the filename (in subdirectory) for a hunk.
fn path_for_hunk(dir: &Path, hunk_number: u32) -> PathBuf {
    let mut buf = subdir_for_hunk(dir, hunk_number);
    buf.push(format!("{:09}", hunk_number));
    buf
}


/// Read out all the entries from an existing index.
pub struct Iter {
    /// The `i` directory within the band where all files for this index are written.
    dir: PathBuf,
}


/// Create an iterator that will read all entires from an existing index.
pub fn read(dir: &Path) -> io::Result<Iter> {
    Ok(Iter {
        dir: dir.to_path_buf(),
    })
}


impl Iterator for Iter {
    type Item = io::Result<IndexEntry>;

    fn next(&mut self) -> Option<io::Result<IndexEntry>> {
        // TODO: Read and decompress and deserialize the next whole file.
        let hunk_path = path_for_hunk(&self.dir, 0);
        None
    }
}


#[cfg(test)]
mod tests {
    use rustc_serialize::json;
    use std::path::{Path};
    use tempdir;

    use super::{IndexBuilder, IndexEntry, IndexKind};
    use super::super::io::read_and_decompress;
    use super::super::report::Report;

    const EXAMPLE_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf21\
         45b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    const ONE_ENTRY_INDEX_JSON: &'static str =             r#"[{"apath":"/hello","mtime":0,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c"}]"#;

    fn scratch_indexbuilder() -> (tempdir::TempDir, IndexBuilder, Report) {
        let testdir = tempdir::TempDir::new("index_test").unwrap();
        let ib = IndexBuilder::new(testdir.path());
        (testdir, ib, Report::new())
    }

    #[test]
    fn serialize_index() {
        let entries = [IndexEntry {
            apath: "/a/b".to_string(),
            mtime: 1461736377,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        }];
        let index_json = json::encode(&entries).unwrap();
        println!("{}", index_json);
        assert_eq!(
            index_json,
            r#"[{"apath":"/a/b","mtime":1461736377,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c"}]"#);
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_order() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        ib.push(IndexEntry {
            apath: "/zzz".to_string(),
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
    fn index_builder_checks_names() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        ib.push(IndexEntry {
            apath: "../escapecat".to_string(),
            mtime: 0,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        })
    }

    fn add_an_entry(ib: &mut IndexBuilder) {
        ib.push(IndexEntry {
            apath: "/hello".to_string(),
            mtime: 0,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        });
    }

    #[test]
    fn index_to_json() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        add_an_entry(&mut ib);
        let json = ib.to_json();
        assert_eq!(json, ONE_ENTRY_INDEX_JSON);
    }

    #[test]
    fn path_for_hunk() {
        let index_dir = Path::new("/foo");
        let hunk_path = super::path_for_hunk(&index_dir, 0);
        assert_eq!(file_name_as_str(&hunk_path), "000000000");
        assert_eq!(last_dir_name_as_str(&hunk_path), "00000");
    }

    fn file_name_as_str(p: &Path) -> &str {
        p.file_name().unwrap().to_str().unwrap()
    }

    fn last_dir_name_as_str(p: &Path) -> &str {
        p.parent().unwrap().file_name().unwrap().to_str().unwrap()
    }

    #[test]
    fn write_a_hunk() {
        use std::str;

        let (_testdir, mut ib, mut report) = scratch_indexbuilder();
        add_an_entry(&mut ib);
        ib.finish_hunk(&mut report).unwrap();

        // The first hunk exists.
        let mut expected_path = ib.dir.to_path_buf();
        expected_path.push("00000");
        expected_path.push("000000000");

        let retrieved_bytes = read_and_decompress(&expected_path).unwrap();
        let retrieved = str::from_utf8(&retrieved_bytes).unwrap();
        assert_eq!(retrieved, ONE_ENTRY_INDEX_JSON);

        let mut it = super::read(&ib.dir).unwrap();
        // TODO: Test that it has a single entry.
        // let entry = it.next().unwrap().unwrap();
        assert!(it.next().is_none(), "No more entries");
    }

    #[test]
    #[should_panic]
    fn no_duplicate_paths() {
        let (_testdir, mut ib, mut _report) = scratch_indexbuilder();
        add_an_entry(&mut ib);
        add_an_entry(&mut ib);
    }

    #[test]
    #[should_panic]
    fn no_duplicate_paths_across_hunks() {
        let (_testdir, mut ib, mut report) = scratch_indexbuilder();
        add_an_entry(&mut ib);
        ib.finish_hunk(&mut report).unwrap();

        // Try to add an identically-named file within the next hunk and it should error,
        // because the IndexBuilder remembers the last file name written.
        add_an_entry(&mut ib);
    }
}
