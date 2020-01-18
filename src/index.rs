// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Index lists the files in a band in the archive.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str;
use std::vec;

use globset::GlobSet;
use snafu::ResultExt;

use super::io::file_exists;
use super::*;

pub const MAX_ENTRIES_PER_HUNK: usize = 1000;

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
    check_order: apath::CheckOrder,
}

/// Accumulate and write out index entries into files in an index directory.
impl IndexBuilder {
    /// Make a new builder that will write files into the given directory.
    pub fn new(dir: &Path) -> IndexBuilder {
        IndexBuilder {
            dir: dir.to_path_buf(),
            entries: Vec::<Entry>::with_capacity(MAX_ENTRIES_PER_HUNK),
            sequence: 0,
            check_order: apath::CheckOrder::new(),
        }
    }

    /// Append an entry to the index.
    ///
    /// The new entry must sort after everything already written to the index.
    pub fn push(&mut self, entry: Entry) {
        // We do this check here rather than the Index constructor so that we
        // can still read invalid apaths...
        self.check_order.check(&entry.apath);
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
        ensure_dir_exists(&subdir_for_hunk(&self.dir, self.sequence))
            .context(errors::WriteIndex)?;
        let hunk_path = &path_for_hunk(&self.dir, self.sequence);

        let json_string = serde_json::to_string(&self.entries)
            .context(errors::SerializeJson { path: hunk_path })?;
        let uncompressed_len = json_string.len() as u64;

        let mut af = AtomicFile::new(hunk_path).context(errors::WriteIndex)?;
        let compressed_len = Snappy::compress_and_write(json_string.as_bytes(), &mut af)
            .context(errors::WriteIndex)?;

        // TODO: Don't seek, just count bytes as they're compressed.
        // TODO: Measure time to compress separately from time to write.
        af.close(report).context(errors::WriteIndex)?;

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

#[derive(Debug, Clone)]
pub struct ReadIndex {
    dir: PathBuf,
}

impl ReadIndex {
    pub fn new(dir: &Path) -> ReadIndex {
        ReadIndex {
            dir: dir.to_path_buf(),
        }
    }

    /// Return the (1-based) number of index hunks in an index directory.
    pub fn count_hunks(&self) -> Result<u32> {
        for i in 0.. {
            if !file_exists(&path_for_hunk(&self.dir, i)).context(errors::ReadIndex)? {
                // If hunk 1 is missing, 1 hunks exists.
                return Ok(i);
            }
        }
        unreachable!();
    }

    pub fn estimate_entry_count(&self) -> Result<u64> {
        Ok(u64::from(self.count_hunks()?) * (MAX_ENTRIES_PER_HUNK as u64))
    }

    /// Make an iterator that will return all entries in this band.
    pub fn iter(&self, excludes: &GlobSet, report: &Report) -> Result<index::Iter> {
        index::Iter::open(&self.dir, excludes, report)
    }
}

/// Read out all the entries from an existing index, continuing across multiple
/// hunks.
pub struct Iter {
    /// The `i` directory within the band where all files for this index are written.
    dir: PathBuf,
    buffered_entries: vec::IntoIter<Entry>,
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
    type Item = Entry;

    fn next(&mut self) -> Option<Entry> {
        loop {
            if let Some(entry) = self.buffered_entries.next() {
                return Some(entry);
            }
            // TODO: refill_entry_buffer shouldn't return a Result.
            if !self.refill_entry_buffer().unwrap() {
                return None; // No more hunks
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
            buffered_entries: Vec::<Entry>::new().into_iter(),
            next_hunk_number: 0,
            report: report.clone(),
            excludes: excludes.clone(),
        })
    }

    /// Read another hunk file and put it into buffered_entries.
    /// Returns true if another hunk could be found, otherwise false.
    /// (It's possible though unlikely the hunks can be empty.)
    fn refill_entry_buffer(&mut self) -> Result<bool> {
        // Load the next index hunk into buffered_entries.
        let hunk_path = path_for_hunk(&self.dir, self.next_hunk_number);
        let mut f = match fs::File::open(&hunk_path) {
            Ok(f) => f,
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                // No (more) index hunk files.
                return Ok(false);
            }
            Err(e) => return Err(e).context(errors::ReadIndex),
        };
        let (comp_len, index_bytes) = Snappy::decompress_read(&mut f).context(errors::ReadIndex)?;
        self.report.increment_size(
            "index",
            Sizes {
                uncompressed: index_bytes.len() as u64,
                compressed: comp_len as u64,
            },
        );
        self.report.increment("index.hunk", 1);

        // TODO: More specific error messages including the filename.
        let index_json = str::from_utf8(&index_bytes).map_err(|_e| Error::IndexCorrupt {
            path: hunk_path.clone(),
        })?;
        let entries: Vec<Entry> =
            serde_json::from_str(index_json).with_context(|| errors::DeserializeIndex {
                path: hunk_path.clone(),
            })?;
        if entries.is_empty() {
            self.report
                .problem(&format!("Index hunk {} is empty", hunk_path.display()));
        }

        self.buffered_entries = entries
            .into_iter()
            .filter(|entry| {
                if self.excludes.is_match(Path::new(&entry.apath.to_string())) {
                    match entry.kind() {
                        Kind::Dir => self.report.increment("skipped.excluded.directories", 1),
                        Kind::Symlink => self.report.increment("skipped.excluded.symlinks", 1),
                        Kind::File => self.report.increment("skipped.excluded.files", 1),
                        Kind::Unknown => self.report.increment("skipped.excluded.unknown", 1),
                    }
                    false
                } else {
                    true
                }
            })
            .collect::<Vec<Entry>>()
            .into_iter();

        self.next_hunk_number += 1;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use crate::*;

    pub fn scratch_indexbuilder() -> (TempDir, IndexBuilder, Report) {
        let testdir = TempDir::new().unwrap();
        let ib = IndexBuilder::new(testdir.path());
        (testdir, ib, Report::new())
    }

    pub fn add_an_entry(ib: &mut IndexBuilder, apath: &str) {
        ib.push(Entry {
            apath: apath.into(),
            mtime: None,
            kind: Kind::File,
            addrs: vec![],
            target: None,
            size: Some(0),
        });
    }

    #[test]
    fn serialize_index() {
        let entries = [Entry {
            apath: "/a/b".into(),
            mtime: Some(1_461_736_377),
            kind: Kind::File,
            addrs: vec![],
            target: None,
            size: Some(0),
        }];
        let index_json = serde_json::to_string(&entries).unwrap();
        println!("{}", index_json);
        assert_eq!(
            index_json,
            "[{\"apath\":\"/a/b\",\
             \"kind\":\"File\",\
             \"mtime\":1461736377}]"
        );
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_order() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        ib.push(Entry {
            apath: "/zzz".into(),
            mtime: None,
            kind: Kind::File,
            addrs: vec![],
            target: None,
            size: Some(0),
        });
        ib.push(Entry {
            apath: "aaa".into(),
            mtime: None,
            kind: Kind::File,
            addrs: vec![],
            target: None,
            size: Some(0),
        });
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_names() {
        let (_testdir, mut ib, _report) = scratch_indexbuilder();
        ib.push(Entry {
            apath: "../escapecat".into(),
            mtime: None,
            kind: Kind::File,
            addrs: vec![],
            target: None,
            size: Some(0),
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
             \"kind\":\"File\"},\
             {\"apath\":\"/banana\",\
             \"kind\":\"File\"}]"
        );

        let mut it = super::Iter::open(&ib.dir, &excludes::excludes_nothing(), &report).unwrap();
        let entry = it.next().expect("Get first entry");
        assert_eq!(&entry.apath, "/apple");
        let entry = it.next().expect("Get second entry");
        assert_eq!(&entry.apath, "/banana");
        assert!(it.next().is_none(), "Expected no more entries");
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

        let names: Vec<String> = it.map(|x| x.apath.into()).collect();
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

        let names: Vec<String> = it.map(|x| x.apath.into()).collect();
        assert_eq!(names, &["/bar"]);
    }
}
