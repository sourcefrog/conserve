// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Listing of files in a band in the archive.

use std::fmt;
use std::io;
use std::io::SeekFrom;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::str;
use std::time;
use std::vec;

use rustc_serialize::json;
use brotli2::write::BrotliEncoder;

use super::apath::Apath;
use super::block;
use super::errors::*;
use super::io::{AtomicFile, ensure_dir_exists, read_and_decompress};
use super::report::{Report, Sizes};


const MAX_ENTRIES_PER_HUNK: usize = 1000;


/// Kind of file that can be stored in the archive.
#[derive(Debug, PartialEq, RustcDecodable, RustcEncodable)]
pub enum IndexKind {
    File,
    Dir,
    Symlink,
}


/// Description of one archived file.
#[derive(Debug, RustcDecodable, RustcEncodable)]
pub struct Entry {
    /// Path of this entry relative to the base of the backup, in `apath` form.
    pub apath: String,

    /// File modification time, in whole seconds past the Unix epoch.
    pub mtime: Option<u64>,

    /// Type of file.
    pub kind: IndexKind,

    /// BLAKE2b hash of the entire original file, without salt.
    pub blake2b: Option<String>,

    /// Blocks holding the file contents.
    pub addrs: Vec<block::Address>,

    /// For symlinks only, the target of the symlink.
    pub target: Option<String>,
}


/// Accumulates ordered changes to the index and streams them out to index files.
#[derive(Debug)]
pub struct IndexBuilder {
    /// The `i` directory within the band where all files for this index are written.
    dir: PathBuf,

    /// Currently queued entries to be written out.
    entries: Vec<Entry>,

    /// Index hunk number, starting at 0.
    sequence: u32,

    /// The last-added filename, to enforce ordering.  At the start of the first hunk
    /// this is empty; at the start of a later hunk it's the last path from the previous
    /// hunk, and otherwise it's the last path from `entries`.
    last_apath: Option<Apath>,
}


/// Accumulate and write out index entries into files in an index directory.
impl IndexBuilder {
    /// Make a new builder that will write files into the given directory.
    pub fn new(dir: &Path) -> IndexBuilder {
        IndexBuilder {
            dir: dir.to_path_buf(),
            entries: Vec::<Entry>::new(),
            sequence: 0,
            last_apath: None,
        }
    }

    /// Append an entry to the index.
    ///
    /// The new entry must sort after everything already written to the index.
    pub fn push(&mut self, entry: Entry) {
        // We do this check here rather than the Index constructor so that we
        // can still read invalid apaths...
        let entry_apath = Apath::from_string(&entry.apath);
        if let Some(ref last_apath) = self.last_apath {
            assert!(last_apath < &entry_apath);
        }
        self.last_apath = Some(entry_apath.clone());
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
        try!(ensure_dir_exists(&subdir_for_hunk(&self.dir, self.sequence)));
        let hunk_path = &path_for_hunk(&self.dir, self.sequence);

        let json_string = report.measure_duration("index.encode", || json::encode(&self.entries))
            .unwrap();
        let uncompressed_len = json_string.len() as u64;

        let af = try!(AtomicFile::new(hunk_path));
        let mut encoder = BrotliEncoder::new(af, super::BROTLI_COMPRESSION_LEVEL);

        let start_compress = time::Instant::now();
        try!(encoder.write_all(json_string.as_bytes()));
        let mut af = try!(encoder.finish());
        report.increment_duration("index.compress", start_compress.elapsed());

        // TODO: Don't seek, just count bytes as they're compressed.
        // TODO: Measure time to compress separately from time to write.
        let compressed_len: u64 = try!(af.seek(SeekFrom::Current(0)));
        try!(af.close(report));

        report.increment_size("index",
                              Sizes {
                                  uncompressed: uncompressed_len as u64,
                                  compressed: compressed_len as u64,
                              });
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
    buffered_entries: vec::IntoIter<Entry>,
    next_hunk_number: u32,
    pub report: Report,
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


/// Create an iterator that will read all entires from an existing index.
///
/// Prefer to use `Band::index_iter` instead.
pub fn read(index_dir: &Path, report: &Report) -> Result<Iter> {
    Ok(Iter {
        dir: index_dir.to_path_buf(),
        buffered_entries: Vec::<Entry>::new().into_iter(),
        next_hunk_number: 0,
        report: report.clone(),
    })
}


impl Iterator for Iter {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Result<Entry>> {
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
    /// Read another hunk file and put it into buffered_entries.
    /// Returns true if another hunk could be found, otherwise false.
    /// (It's possible though unlikely the hunks can be empty.)
    fn refill_entry_buffer(&mut self) -> Result<bool> {
        let start_read = time::Instant::now();
        // Load the next index hunk into buffered_entries.
        let hunk_path = path_for_hunk(&self.dir, self.next_hunk_number);
        let (comp_len, index_bytes) = match read_and_decompress(&hunk_path) {
            Ok(i) => i,
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                // No (more) index hunk files.
                return Ok(false);
            }
            Err(e) => {
                return Err(e.into());
            }
        };
        self.report.increment_duration("index.read", start_read.elapsed());
        self.report.increment_size("index",
                                   Sizes {
                                       uncompressed: index_bytes.len() as u64,
                                       compressed: comp_len as u64,
                                   });
        self.report.increment("index.hunk", 1);

        let start_parse = time::Instant::now();
        let index_json = try!(str::from_utf8(&index_bytes)
            .chain_err(|| format!("index file {:?} is not UTF-8", hunk_path)));
        let entries: Vec<Entry> = try!(json::decode(index_json)
            .chain_err(|| format!("couldn't deserialize index hunk {:?}", hunk_path)));
        if entries.is_empty() {
            warn!("Index hunk {} is empty", hunk_path.display());
        }
        self.report.increment_duration("index.parse", start_parse.elapsed());

        self.buffered_entries = entries.into_iter();
        self.next_hunk_number += 1;
        Ok(true)
    }
}


#[cfg(test)]
mod tests {
    use rustc_serialize::json;
    use std::path::Path;
    use tempdir;

    use Report;
    use super::{IndexBuilder, Entry, IndexKind};
    use io::read_and_decompress;

    pub const EXAMPLE_HASH: &'static str = "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    pub fn scratch_indexbuilder() -> (tempdir::TempDir, IndexBuilder, Report) {
        let testdir = tempdir::TempDir::new("index_test").unwrap();
        let ib = IndexBuilder::new(testdir.path());
        (testdir, ib, Report::new())
    }

    pub fn add_an_entry(ib: &mut IndexBuilder, apath: &str) {
        ib.push(Entry {
            apath: apath.to_string(),
            mtime: None,
            kind: IndexKind::File,
            blake2b: Some(EXAMPLE_HASH.to_string()),
            addrs: vec![],
            target: None,
        });
    }

    #[test]
    fn serialize_index() {
        let entries = [Entry {
                           apath: "/a/b".to_string(),
                           mtime: Some(1461736377),
                           kind: IndexKind::File,
                           blake2b: Some(EXAMPLE_HASH.to_string()),
                           addrs: vec![],
                           target: None,
                       }];
        let index_json = json::encode(&entries).unwrap();
        println!("{}", index_json);
        assert_eq!(
            index_json,
            r#"[{"apath":"/a/b","mtime":1461736377,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c","addrs":[],"target":null}]"#);
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_order() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        ib.push(Entry {
            apath: "/zzz".to_string(),
            mtime: None,
            kind: IndexKind::File,
            blake2b: Some(EXAMPLE_HASH.to_string()),
            addrs: vec![],
            target: None,
        });
        ib.push(Entry {
            apath: "aaa".to_string(),
            mtime: None,
            kind: IndexKind::File,
            blake2b: Some(EXAMPLE_HASH.to_string()),
            addrs: vec![],
            target: None,
        });
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_names() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        ib.push(Entry {
            apath: "../escapecat".to_string(),
            mtime: None,
            kind: IndexKind::File,
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
        let (_comp_len, retrieved_bytes) = read_and_decompress(&expected_path).unwrap();
        let retrieved = str::from_utf8(&retrieved_bytes).unwrap();
        assert_eq!(retrieved,  r#"[{"apath":"/apple","mtime":null,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c","addrs":[],"target":null},{"apath":"/banana","mtime":null,"kind":"File","blake2b":"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c","addrs":[],"target":null}]"#);

        let mut it = super::read(&ib.dir, &report).unwrap();
        let entry = it.next().expect("Get first entry").expect("First entry isn't an error");
        assert_eq!(entry.apath, "/apple");
        let entry = it.next().expect("Get second entry").expect("Entry isn't an error");
        assert_eq!(entry.apath, "/banana");
        let opt_entry = it.next();
        if !opt_entry.is_none() {
            panic!("Expected no more entries but got {:?}", opt_entry);
        }
    }

    #[test]
    fn multiple_hunks() {
        use std::str;

        let (_testdir, mut ib, report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/1.1");
        add_an_entry(&mut ib, "/1.2");
        ib.finish_hunk(&report).unwrap();

        add_an_entry(&mut ib, "/2.1");
        add_an_entry(&mut ib, "/2.2");
        ib.finish_hunk(&report).unwrap();

        let it = super::read(&ib.dir, &report).unwrap();
        assert_eq!(format!("{:?}", &it),
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
        let (_testdir, mut ib, report) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/hello");
        ib.finish_hunk(&report).unwrap();

        // Try to add an identically-named file within the next hunk and it should error,
        // because the IndexBuilder remembers the last file name written.
        add_an_entry(&mut ib, "hello");
    }
}

#[cfg(all(feature="bench", test))]
mod bench {
    use test::Bencher;
    use super::tests::{add_an_entry, scratch_indexbuilder};

    #[bench]
    fn write_index(b: &mut Bencher) {
        b.iter(|| {
            let (_testdir, mut ib, report) = scratch_indexbuilder();
            for i in 0..100 {
                add_an_entry(&mut ib, &format!("/banana{:04}", i));
            }
            ib.finish_hunk(&report).unwrap();
        });
    }

    #[bench]
    fn read_index(b: &mut Bencher) {
        let (_testdir, mut ib, report) = scratch_indexbuilder();
        for i in 0..100 {
            add_an_entry(&mut ib, &format!("/banana{:04}", i));
        }
        ib.finish_hunk(&report).unwrap();

        b.iter(|| {
            let it = super::read(&ib.dir, &report).unwrap();
            let _entries: Vec<_> = it.collect();
        });
    }
}
