// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Listing of files in a band in the archive.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str;
use std::time;
use std::vec;

use rustc_serialize::json;

use super::*;
use super::apath::Apath;
use super::block;

use globset::GlobSet;

const MAX_ENTRIES_PER_HUNK: usize = 1000;


/// Description of one archived file.
///
/// This struct is directly encoded/decoded to the json index file.
#[derive(Debug, RustcDecodable, RustcEncodable)]
pub struct IndexEntry {
    /// Path of this entry relative to the base of the backup, in `apath` form.
    pub apath: String,

    /// File modification time, in whole seconds past the Unix epoch.
    pub mtime: Option<u64>,

    /// Type of file.
    pub kind: Kind,

    /// BLAKE2b hash of the entire original file, without salt.
    pub blake2b: Option<String>,

    /// Blocks holding the file contents.
    pub addrs: Vec<block::Address>,

    /// For symlinks only, the target of the symlink.
    pub target: Option<String>,
}


impl entry::Entry for IndexEntry {
    fn apath(&self) -> Apath {
        Apath::from_string(&self.apath)
    }

    fn kind(&self) -> Kind {
        self.kind
    }

    fn unix_mtime(&self) -> Option<u64> {
        self.mtime
    }

    fn symlink_target(&self) -> Option<String> {
        assert_eq!(
            self.kind() == Kind::Symlink,
            self.target.is_some());
        self.target.clone()
    }
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
    check_order: apath::CheckOrder,
}


/// Accumulate and write out index entries into files in an index directory.
impl IndexBuilder {
    /// Make a new builder that will write files into the given directory.
    pub fn new(dir: &Path) -> IndexBuilder {
        IndexBuilder {
            dir: dir.to_path_buf(),
            entries: Vec::<IndexEntry>::new(),
            sequence: 0,
            check_order: apath::CheckOrder::new(),
        }
    }

    /// Append an entry to the index.
    ///
    /// The new entry must sort after everything already written to the index.
    pub fn push(&mut self, entry: IndexEntry) {
        // We do this check here rather than the Index constructor so that we
        // can still read invalid apaths...
        self.check_order.check(&Apath::from_string(&entry.apath));
        self.entries.push(entry);
    }

    pub fn maybe_flush(&mut self, report: &Report) -> Result<()> {
        if self.entries.len() >= MAX_ENTRIES_PER_HUNK {
            self.finish_hunk(report)
        } else {
            Ok(())
        }
    }

    /// Finish this hunk of the index.
    ///
    /// This writes all the currently queued entries into a new index file
    /// in the band directory, and then clears the index to start receiving
    /// entries for the next hunk.
    pub fn finish_hunk(&mut self, report: &Report) -> Result<()> {
        ensure_dir_exists(&subdir_for_hunk(&self.dir, self.sequence))?;
        let hunk_path = &path_for_hunk(&self.dir, self.sequence);

        let json_string = report
            .measure_duration("index.encode", || json::encode(&self.entries))
            .unwrap();
        let uncompressed_len = json_string.len() as u64;

        let mut af = AtomicFile::new(hunk_path)?;
        let compressed_len = report.measure_duration("index.compress", || {
            Snappy::compress_and_write(json_string.as_bytes(), &mut af)
        })?;

        // TODO: Don't seek, just count bytes as they're compressed.
        // TODO: Measure time to compress separately from time to write.
        af.close(report)?;

        report.increment_size(
            "index",
            Sizes {
                uncompressed: uncompressed_len as u64,
                compressed: compressed_len as u64,
            },
        );
        report.increment("index.hunk", 1);

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
    pub report: Report,
    excludes: GlobSet,
}


impl fmt::Debug for Iter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("index::Iter")
            .field("dir", &self.dir)
            .field("next_hunk_number", &self.next_hunk_number)
            // .field("report", &self.report)
            // buffered_entries has no Debug itself
            .finish()
    }
}


impl Iterator for Iter {
    type Item = Result<IndexEntry>;

    fn next(&mut self) -> Option<Result<IndexEntry>> {
        loop {
            if let Some(entry) = self.buffered_entries.next() {
                return Some(Ok(entry));
            }
            match self.refill_entry_buffer() {
                Err(e) => return Some(Err(e)),
                Ok(false) => return None, // No more hunks
                Ok(true) => (),
            }
        }
    }
}

impl Iter {
    /// Create an iterator that will read all entires from an existing index.
    ///
    /// Prefer to use `Band::index_iter` instead.
    pub fn open(index_dir: &Path, excludes: &GlobSet, report: &Report) -> Result<Iter> {
        Ok(Iter {
            dir: index_dir.to_path_buf(),
            buffered_entries: Vec::<IndexEntry>::new().into_iter(),
            next_hunk_number: 0,
            report: report.clone(),
            excludes: excludes.clone(),
        })
    }

    /// Read another hunk file and put it into buffered_entries.
    /// Returns true if another hunk could be found, otherwise false.
    /// (It's possible though unlikely the hunks can be empty.)
    fn refill_entry_buffer(&mut self) -> Result<bool> {
        let start_read = time::Instant::now();
        // Load the next index hunk into buffered_entries.
        let hunk_path = path_for_hunk(&self.dir, self.next_hunk_number);
        let mut f = match fs::File::open(&hunk_path) {
            Ok(f) => f,
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                // No (more) index hunk files.
                return Ok(false);
            }
            Err(e) => {
                return Err(e.into());
            }
        };
        let (comp_len, index_bytes) = Snappy::decompress_read(&mut f)?;
        self.report.increment_duration(
            "index.read",
            start_read.elapsed(),
        );
        self.report.increment_size(
            "index",
            Sizes {
                uncompressed: index_bytes.len() as u64,
                compressed: comp_len as u64,
            },
        );
        self.report.increment("index.hunk", 1);

        let start_parse = time::Instant::now();
        let index_json = str::from_utf8(&index_bytes).chain_err(|| {
            format!("index file {:?} is not UTF-8", hunk_path)
        })?;
        let entries: Vec<IndexEntry> = json::decode(index_json).chain_err(|| {
            format!("couldn't deserialize index hunk {:?}", hunk_path)
        })?;
        if entries.is_empty() {
            warn!("Index hunk {} is empty", hunk_path.display());
        }
        self.report.increment_duration(
            "index.parse",
            start_parse.elapsed(),
        );

        self.buffered_entries = entries
            .into_iter()
            .filter(|entry| {
                if self.excludes.is_match(&entry.apath) {
                    match entry.kind() {
                        Kind::Dir => self.report.increment("skipped.excluded.directories", 1),
                        Kind::Symlink => self.report.increment("skipped.excluded.symlinks", 1),
                        Kind::File => self.report.increment("skipped.excluded.files", 1),
                        Kind::Unknown => self.report.increment("skipped.excluded.unknown", 1),
                    }
                    return false;
                }
                return true;
            })
            .collect::<Vec<IndexEntry>>()
            .into_iter();

        self.next_hunk_number += 1;
        Ok(true)
    }
}


#[cfg(test)]
mod tests {
    use rustc_serialize::json;
    use std::path::Path;
    use tempdir;

    use super::super::*;

    pub const EXAMPLE_HASH: &'static str = "66ad1939a9289aa9f1f1d9ad7bcee69429\
        3c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b\
        2fb1d67e28262168013ba63c";

    pub fn scratch_indexbuilder() -> (tempdir::TempDir, IndexBuilder, Report) {
        let testdir = tempdir::TempDir::new("index_test").unwrap();
        let ib = IndexBuilder::new(testdir.path());
        (testdir, ib, Report::new())
    }

    pub fn add_an_entry(ib: &mut IndexBuilder, apath: &str) {
        ib.push(IndexEntry {
            apath: apath.to_string(),
            mtime: None,
            kind: Kind::File,
            blake2b: Some(EXAMPLE_HASH.to_string()),
            addrs: vec![],
            target: None,
        });
    }

    #[test]
    fn serialize_index() {
        let entries = [
            IndexEntry {
                apath: "/a/b".to_string(),
                mtime: Some(1461736377),
                kind: Kind::File,
                blake2b: Some(EXAMPLE_HASH.to_string()),
                addrs: vec![],
                target: None,
            },
        ];
        let index_json = json::encode(&entries).unwrap();
        println!("{}", index_json);
        assert_eq!(
            index_json,
            "[{\"apath\":\"/a/b\",\
            \"mtime\":1461736377,\
            \"kind\":\"File\",\
            \"blake2b\":\"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3\
            f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262\
            168013ba63c\",\
            \"addrs\":[],\
            \"target\":null}]"
        );
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_order() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        ib.push(IndexEntry {
            apath: "/zzz".to_string(),
            mtime: None,
            kind: Kind::File,
            blake2b: Some(EXAMPLE_HASH.to_string()),
            addrs: vec![],
            target: None,
        });
        ib.push(IndexEntry {
            apath: "aaa".to_string(),
            mtime: None,
            kind: Kind::File,
            blake2b: Some(EXAMPLE_HASH.to_string()),
            addrs: vec![],
            target: None,
        });
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_names() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        ib.push(IndexEntry {
            apath: "../escapecat".to_string(),
            mtime: None,
            kind: Kind::File,
            blake2b: Some(EXAMPLE_HASH.to_string()),
            addrs: vec![],
            target: None,
        })
    }

    #[test]
    fn path_for_hunk() {
        let index_dir = Path::new("/foo");
        let hunk_path = super::path_for_hunk(index_dir, 0);
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
        use std::fs;
        use std::str;

        let (_testdir, mut ib, report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/apple");
        add_an_entry(&mut ib, "/banana");
        ib.finish_hunk(&report).unwrap();

        // The first hunk exists.
        let mut expected_path = ib.dir.to_path_buf();
        expected_path.push("00000");
        expected_path.push("000000000");

        // Check the stored json version
        let mut f = fs::File::open(&expected_path).unwrap();
        let (_comp_len, retrieved_bytes) = Snappy::decompress_read(&mut f).unwrap();
        let retrieved = str::from_utf8(&retrieved_bytes).unwrap();
        assert_eq!(
            retrieved,
            "[{\"apath\":\"/apple\",\
            \"mtime\":null,\
            \"kind\":\"File\",\
            \"blake2b\":\"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb\
            5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c\
            2b2fb1d67e28262168013ba63c\",\
            \"addrs\":[],\
            \"target\":null},\
            {\"apath\":\"/banana\",\
            \"mtime\":null,\
            \"kind\":\"File\",\
            \"blake2b\":\"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb\
            5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c\
            2b2fb1d67e28262168013ba63c\",\
            \"addrs\":[],\
            \"target\":null}]"
        );

        let mut it = super::Iter::open(&ib.dir, &excludes::excludes_nothing(), &report).unwrap();
        let entry = it.next().expect("Get first entry").expect(
            "First entry isn't an error",
        );
        assert_eq!(entry.apath, "/apple");
        let entry = it.next().expect("Get second entry").expect(
            "IndexEntry isn't an error",
        );
        assert_eq!(entry.apath, "/banana");
        let opt_entry = it.next();
        if !opt_entry.is_none() {
            panic!("Expected no more entries but got {:?}", opt_entry);
        }
    }

    #[test]
    fn multiple_hunks() {
        let (_testdir, mut ib, report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/1.1");
        add_an_entry(&mut ib, "/1.2");
        ib.finish_hunk(&report).unwrap();

        add_an_entry(&mut ib, "/2.1");
        add_an_entry(&mut ib, "/2.2");
        ib.finish_hunk(&report).unwrap();

        let it = super::Iter::open(&ib.dir, &excludes::excludes_nothing(), &report).unwrap();
        assert_eq!(
            format!("{:?}", &it),
            format!("index::Iter {{ dir: {:?}, next_hunk_number: 0 }}", ib.dir)
        );

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
        let (_testdir, mut ib, report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/hello");
        ib.finish_hunk(&report).unwrap();

        // Try to add an identically-named file within the next hunk and it should error,
        // because the IndexBuilder remembers the last file name written.
        add_an_entry(&mut ib, "hello");
    }

    #[test]
    fn excluded_entries() {
        let (_testdir, mut ib, report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/bar");
        add_an_entry(&mut ib, "/foo");
        add_an_entry(&mut ib, "/foobar");
        ib.finish_hunk(&report).unwrap();

        let excludes = excludes::from_strings(&["/fo*"]).unwrap();
        let it = super::Iter::open(&ib.dir, &excludes, &report).unwrap();
        assert_eq!(
            format!("{:?}", &it),
            format!("index::Iter {{ dir: {:?}, next_hunk_number: 0 }}", ib.dir)
        );

        let names: Vec<String> = it.map(|x| x.unwrap().apath).collect();
        assert_eq!(names, &["/bar"]);
    }
}
