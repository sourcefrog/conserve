// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

///! Listing of files in a band in the archive.

use std::cmp::Ordering;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::str;
use std::vec;

use rustc_serialize::json;

use super::apath;
use super::io::{read_and_decompress, write_compressed_bytes};
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

    /// Finish this hunk of the index.
    ///
    /// This writes all the currently queued entries into a new index file
    /// in the band directory, and then clears the index to start receiving
    /// entries for the next hunk.
    pub fn finish_hunk(&mut self, report: &mut Report) -> io::Result<()> {
        let json_str = json::encode(&self.entries).unwrap();
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
    buffered_entries: vec::IntoIter<IndexEntry>,
    next_hunk_number: u32,
}


impl fmt::Debug for Iter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("index::Iter")
            .field("dir", &self.dir)
            .field("next_hunk_number", &self.next_hunk_number)
            // buffered_entries has no Debug itself
            .finish()
    }
}


/// Create an iterator that will read all entires from an existing index.
pub fn read(index_dir: &Path) -> io::Result<Iter> {
    Ok(Iter {
        dir: index_dir.to_path_buf(),
        buffered_entries: Vec::<IndexEntry>::new().into_iter(),
        next_hunk_number: 0,
    })
}


impl Iterator for Iter {
    type Item = io::Result<IndexEntry>;

    fn next(&mut self) -> Option<io::Result<IndexEntry>> {
        loop {
            if let Some(entry) = self.buffered_entries.next() {
                return Some(Ok(entry));
            }
            // Load the next index hunk into buffered_entries.
            let hunk_path = path_for_hunk(&self.dir, self.next_hunk_number);
            let index_bytes = match read_and_decompress(&hunk_path) {
                Ok(i) => i,
                Err(e) => {
                    if e.kind() == io::ErrorKind::NotFound {
                        // No (more) index hunk files.
                        return None;
                    } else {
                        return Some(Err(e));
                    }
                },
            };
            let index_json = match str::from_utf8(&index_bytes) {
                Ok(s) => s,
                Err(e) => {
                    error!("Index file {} is not UTF-8: {}", hunk_path.display(), e);
                    return Some(Err(io::Error::new(io::ErrorKind::InvalidInput, e)));
                },
            };
            let entries: Vec<IndexEntry> = match json::decode(&index_json) {
                Ok(h) => h,
                Err(e) => {
                    error!("Couldn't deserialize index hunk {}: {}", hunk_path.display(), e);
                    return Some(Err(io::Error::new(io::ErrorKind::InvalidInput, e)));
                }
            };
            if entries.is_empty() {
                warn!("Index hunk {} is empty", hunk_path.display());
            }
            self.buffered_entries = entries.into_iter();
            self.next_hunk_number += 1;
        }
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

    fn add_an_entry(ib: &mut IndexBuilder, apath: &str) {
        ib.push(IndexEntry {
            apath: apath.to_string(),
            mtime: 0,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        });
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
    fn basic() {
        use std::str;

        let (_testdir, mut ib, mut report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/apple");
        add_an_entry(&mut ib, "/banana");
        ib.finish_hunk(&mut report).unwrap();

        // The first hunk exists.
        let mut expected_path = ib.dir.to_path_buf();
        expected_path.push("00000");
        expected_path.push("000000000");

        // Check the stored json version
        let retrieved_bytes = read_and_decompress(&expected_path).unwrap();
        let retrieved = str::from_utf8(&retrieved_bytes).unwrap();
        assert_eq!(retrieved,  r#"[{"apath":"/apple","mtime":0,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c"},{"apath":"/banana","mtime":0,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c"}]"#);

        let mut it = super::read(&ib.dir).unwrap();
        let entry = it.next().expect("Get first entry").expect("First entry isn't an error");
        assert_eq!(entry.apath, "/apple");
        let entry = it.next().expect("Get second entry").expect("Entry isn't an error");
        assert_eq!(entry.apath, "/banana");
        let opt_entry = it.next();
        if ! opt_entry.is_none() {
            panic!("Expected no more entries but got {:?}", opt_entry);
        }
    }

    #[test]
    fn multiple_hunks() {
        use std::str;

        let (_testdir, mut ib, mut report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/1.1");
        add_an_entry(&mut ib, "/1.2");
        ib.finish_hunk(&mut report).unwrap();

        add_an_entry(&mut ib, "/2.1");
        add_an_entry(&mut ib, "/2.2");
        ib.finish_hunk(&mut report).unwrap();

        let it = super::read(&ib.dir).unwrap();
        assert_eq!(
            format!("{:?}", &it),
            format!("index::Iter {{ dir: {:?}, next_hunk_number: 0 }}", ib.dir));

        let names: Vec<String> = it.map(|x| x.unwrap().apath).collect();
        assert_eq!(names, &["/1.1", "/1.2", "/2.1", "/2.2"]);
    }

    #[test]
    #[should_panic]
    fn no_duplicate_paths() {
        let (_testdir, mut ib, mut _report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/hello");
        add_an_entry(&mut ib, "/hello");
    }

    #[test]
    #[should_panic]
    fn no_duplicate_paths_across_hunks() {
        let (_testdir, mut ib, mut report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/hello");
        ib.finish_hunk(&mut report).unwrap();

        // Try to add an identically-named file within the next hunk and it should error,
        // because the IndexBuilder remembers the last file name written.
        add_an_entry(&mut ib, "hello");
    }
}
