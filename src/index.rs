// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Index lists the files in a band in the archive.

use std::cmp::Ordering;
use std::io;
use std::iter::Peekable;
use std::path::Path;
use std::vec;

use globset::GlobSet;

use crate::compress::snappy::{Compressor, Decompressor};
use crate::kind::Kind;
use crate::stats::{IndexBuilderStats, IndexEntryIterStats};
use crate::transport::local::LocalTransport;
use crate::transport::Transport;
use crate::unix_time::UnixTime;
use crate::*;

pub const MAX_ENTRIES_PER_HUNK: usize = 1000;

pub const HUNKS_PER_SUBDIR: u32 = 10_000;

/// Description of one archived file.
///
/// This struct is directly encoded/decoded to the json index file, and also can be constructed by
/// stat-ing (but not reading) a live file.
#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IndexEntry {
    /// Path of this entry relative to the base of the backup, in `apath` form.
    pub apath: Apath,

    /// Type of file.
    pub kind: Kind,

    /// File modification time, in whole seconds past the Unix epoch.
    #[serde(default)]
    pub mtime: i64,

    /// Fractional nanoseconds for modification time.
    ///
    /// This is zero in indexes written prior to 0.6.2, but treating it as
    /// zero is harmless - around the transition files will be seen as
    /// potentially touched.
    ///
    /// It seems moderately common that the nanos are zero, probably because
    /// the time was set by something that didn't preserve them. In that case,
    /// skip serializing.
    #[serde(default)]
    #[serde(skip_serializing_if = "crate::misc::zero_u32")]
    pub mtime_nanos: u32,

    /// For stored files, the blocks holding the file contents.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addrs: Vec<blockdir::Address>,

    /// For symlinks only, the target of the symlink.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl Entry for IndexEntry {
    /// Return apath relative to the top of the tree.
    fn apath(&self) -> &Apath {
        &self.apath
    }

    #[inline]
    fn kind(&self) -> Kind {
        self.kind
    }

    #[inline]
    fn mtime(&self) -> UnixTime {
        UnixTime {
            secs: self.mtime,
            nanosecs: self.mtime_nanos,
        }
    }

    /// Size of the file, if it is a file. None for directories and symlinks.
    fn size(&self) -> Option<u64> {
        Some(self.addrs.iter().map(|a| a.len).sum())
    }

    /// Target of the symlink, if this is a symlink.
    #[inline]
    fn symlink_target(&self) -> &Option<String> {
        &self.target
    }
}

impl IndexEntry {
    /// Copy the metadata, but not the body content, from another entry.
    pub(crate) fn metadata_from<E: Entry>(source: &E) -> IndexEntry {
        let mtime = source.mtime();
        assert_eq!(
            source.symlink_target().is_some(),
            source.kind() == Kind::Symlink
        );
        IndexEntry {
            apath: source.apath().clone(),
            kind: source.kind(),
            addrs: Vec::new(),
            target: source.symlink_target().clone(),
            mtime: mtime.secs,
            mtime_nanos: mtime.nanosecs,
        }
    }
}

/// Accumulates ordered changes to the index and streams them out to index files.
pub struct IndexBuilder {
    /// The `i` directory within the band where all files for this index are written.
    transport: Box<dyn Transport>,

    /// Currently queued entries to be written out.
    entries: Vec<IndexEntry>,

    /// Index hunk number, starting at 0.
    sequence: u32,

    /// The last-added filename, to enforce ordering.  At the start of the first hunk
    /// this is empty; at the start of a later hunk it's the last path from the previous
    /// hunk, and otherwise it's the last path from `entries`.
    check_order: apath::CheckOrder,

    /// Statistics about work done while writing this index.
    pub stats: IndexBuilderStats,

    compressor: Compressor,
}

/// Accumulate and write out index entries into files in an index directory.
impl IndexBuilder {
    /// Make a new builder that will write files into the given directory.
    pub fn new(transport: Box<dyn Transport>) -> IndexBuilder {
        IndexBuilder {
            transport,
            entries: Vec::<IndexEntry>::with_capacity(MAX_ENTRIES_PER_HUNK),
            sequence: 0,
            check_order: apath::CheckOrder::new(),
            stats: IndexBuilderStats::default(),
            compressor: Compressor::new(),
        }
    }

    pub fn finish(mut self) -> Result<IndexBuilderStats> {
        self.finish_hunk()?;
        Ok(self.stats)
    }

    /// Append an entry to the index.
    ///
    /// The new entry must sort after everything already written to the index.
    pub(crate) fn push_entry(&mut self, entry: IndexEntry) -> Result<()> {
        // We do this check here rather than the Index constructor so that we
        // can still read invalid apaths...
        self.check_order.check(&entry.apath);
        self.entries.push(entry);
        if self.entries.len() >= MAX_ENTRIES_PER_HUNK {
            self.finish_hunk()
        } else {
            Ok(())
        }
    }

    /// Finish this hunk of the index.
    ///
    /// This writes all the currently queued entries into a new index file
    /// in the band directory, and then clears the buffer to start receiving
    /// entries for the next hunk.
    fn finish_hunk(&mut self) -> Result<()> {
        if self.entries.is_empty() {
            return Ok(());
        }

        let relpath = hunk_relpath(self.sequence);
        let write_error = |source| Error::WriteIndex {
            path: relpath.clone(),
            source,
        };
        let json =
            serde_json::to_vec(&self.entries).map_err(|source| Error::SerializeIndex { source })?;
        let uncompressed_len = json.len() as u64;
        if (self.sequence % HUNKS_PER_SUBDIR) == 0 {
            self.transport
                .create_dir(&subdir_relpath(self.sequence))
                .map_err(write_error)?;
        }
        let compressed_bytes = self.compressor.compress(&json)?;
        let compressed_len = compressed_bytes.len();
        self.transport
            .write_file(&relpath, compressed_bytes)
            .map_err(write_error)?;

        self.stats.index_hunks += 1;
        self.stats.compressed_index_bytes += compressed_len as u64;
        self.stats.uncompressed_index_bytes += uncompressed_len as u64;
        self.entries.clear(); // Ready for the next hunk.
        self.sequence += 1;
        Ok(())
    }
}

/// Return the transport-relative path for a subdirectory.
fn subdir_relpath(hunk_number: u32) -> String {
    format!("{:05}", hunk_number / HUNKS_PER_SUBDIR)
}

/// Return the relative path for a hunk.
fn hunk_relpath(hunk_number: u32) -> String {
    format!("{:05}/{:09}", hunk_number / HUNKS_PER_SUBDIR, hunk_number)
}

#[derive(Debug, Clone)]
pub struct IndexRead {
    /// Transport pointing to this index directory.
    transport: Box<dyn Transport>,
}

impl IndexRead {
    pub(crate) fn open(transport: Box<dyn Transport>) -> IndexRead {
        IndexRead { transport }
    }

    pub(crate) fn open_path(path: &Path) -> IndexRead {
        IndexRead::open(Box::new(LocalTransport::new(path)))
    }

    /// Return the (1-based) number of index hunks in an index directory.
    pub fn count_hunks(&self) -> Result<u32> {
        // TODO: Might be faster to list the directory than to probe for all of them.
        // TODO: Perhaps, list the directories and cope cleanly with
        // one hunk being missing.
        for i in 0.. {
            let path = hunk_relpath(i);
            if !self
                .transport
                .exists(&path)
                .map_err(|source| Error::ReadIndex {
                    source,
                    path: path.into(),
                })?
            {
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
    pub fn iter_entries(&self) -> Result<IndexEntryIter> {
        Ok(IndexEntryIter {
            buffered_entries: Vec::<IndexEntry>::new().into_iter().peekable(),
            next_hunk_number: 0,
            excludes: excludes::excludes_nothing(),
            stats: IndexEntryIterStats::default(),
            decompressor: Decompressor::new(),
            transport: self.transport.box_clone(),
            compressed_buf: Vec::new(),
        })
    }
}

/// Read out all the entries from a stored index, in apath order.
pub struct IndexEntryIter {
    /// Temporarily buffered entries, read from the index files but not yet
    /// returned to the client.
    buffered_entries: Peekable<vec::IntoIter<IndexEntry>>,
    next_hunk_number: u32,
    excludes: GlobSet,
    decompressor: Decompressor,
    pub stats: IndexEntryIterStats,
    /// The `i` directory within the band where all files for this index are written.
    transport: Box<dyn Transport>,
    compressed_buf: Vec<u8>,
}

impl Iterator for IndexEntryIter {
    type Item = IndexEntry;

    fn next(&mut self) -> Option<IndexEntry> {
        loop {
            while let Some(entry) = self.buffered_entries.next() {
                if !self.excludes.is_match(&entry.apath) {
                    return Some(entry);
                }
            }
            if !self.refill_entry_buffer_or_warn() {
                return None;
            }
        }
    }
}

impl IndexEntryIter {
    /// Consume this iterator and return a new one with exclusions.
    pub fn with_excludes(self, excludes: globset::GlobSet) -> IndexEntryIter {
        IndexEntryIter { excludes, ..self }
    }

    /// Return the entry for given apath, if it is present, otherwise None.
    /// It follows this will also return None at the end of the index.
    ///
    /// After this is called, the iter has skipped forward to this apath,
    /// discarding entries for any earlier files. However, even if the apath
    /// is not present, other entries coming after it can still be read.
    pub fn advance_to(&mut self, apath: &Apath) -> Option<IndexEntry> {
        // This takes some care because we don't want to consume the entry
        // that tells us we went too far.
        loop {
            if let Some(cand) = self.buffered_entries.peek() {
                match cand.apath.cmp(apath) {
                    Ordering::Less => {
                        // Discard this and continue looking
                        self.buffered_entries.next().unwrap();
                    }
                    Ordering::Equal => {
                        return Some(self.buffered_entries.next().unwrap());
                    }
                    Ordering::Greater => {
                        // We passed the point where this entry would have been:
                        return None;
                    }
                }
            } else if !self.refill_entry_buffer_or_warn() {
                return None;
            }
        }
    }

    /// Refill entry buffer, converting errors to warnings.
    ///
    /// Returns true if a hunk was read; false at the end.
    fn refill_entry_buffer_or_warn(&mut self) -> bool {
        self.refill_entry_buffer().unwrap_or_else(|e| {
            ui::show_error(&e); // Continue to read next hunk.
            true
        })
    }

    /// Read another hunk file and put it into buffered_entries.
    ///
    /// Returns true if another hunk could be found, otherwise false.
    fn refill_entry_buffer(&mut self) -> Result<bool> {
        assert!(
            self.buffered_entries.next().is_none(),
            "refill_entry_buffer called with non-empty buffer"
        );
        let path = &hunk_relpath(self.next_hunk_number);
        // Whether we succeed or fail, don't try to read this hunk again.
        self.next_hunk_number += 1;
        self.stats.index_hunks += 1;
        if let Err(err) = self.transport.read_file(&path, &mut self.compressed_buf) {
            if err.kind() == io::ErrorKind::NotFound {
                // TODO: Cope with one hunk being missing, while there are still
                // later-numbered hunks. This would require reading the whole
                // list of hunks first.
                return Ok(false);
            } else {
                return Err(Error::ReadIndex {
                    path: path.clone(),
                    source: err,
                });
            }
        };
        self.stats.compressed_index_bytes += self.compressed_buf.len() as u64;
        let index_bytes = self.decompressor.decompress(&self.compressed_buf)?;
        self.stats.uncompressed_index_bytes += index_bytes.len() as u64;
        let entries: Vec<IndexEntry> =
            serde_json::from_slice(&index_bytes).map_err(|source| Error::DeserializeIndex {
                path: path.clone(),
                source,
            })?;
        if entries.is_empty() {
            ui::problem(&format!("Index hunk {:?} is empty", path));
        }
        // NOTE: Not updating 'skipped' counters; here. Questionable value.
        self.buffered_entries = entries.into_iter().peekable();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    pub fn scratch_indexbuilder() -> (TempDir, IndexBuilder) {
        let testdir = TempDir::new().unwrap();
        let ib = IndexBuilder::new(Box::new(LocalTransport::new(testdir.path())));
        (testdir, ib)
    }

    pub fn add_an_entry(ib: &mut IndexBuilder, apath: &str) {
        ib.push_entry(IndexEntry {
            apath: apath.into(),
            mtime: 1_461_736_377,
            mtime_nanos: 0,
            kind: Kind::File,
            addrs: vec![],
            target: None,
        })
        .unwrap();
    }

    #[test]
    fn serialize_index() {
        let entries = [IndexEntry {
            apath: "/a/b".into(),
            mtime: 1_461_736_377,
            mtime_nanos: 0,
            kind: Kind::File,
            addrs: vec![],
            target: None,
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
        let (_testdir, mut ib) = scratch_indexbuilder();
        ib.push_entry(IndexEntry {
            apath: "/zzz".into(),
            mtime: 1_461_736_377,
            mtime_nanos: 0,

            kind: Kind::File,
            addrs: vec![],
            target: None,
        })
        .unwrap();
        ib.push_entry(IndexEntry {
            apath: "aaa".into(),
            mtime: 1_461_736_377,
            mtime_nanos: 0,
            kind: Kind::File,
            addrs: vec![],
            target: None,
        })
        .unwrap();
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_names() {
        let (_testdir, mut ib) = scratch_indexbuilder();
        ib.push_entry(IndexEntry {
            apath: "../escapecat".into(),
            mtime: 1_461_736_377,
            kind: Kind::File,
            addrs: vec![],
            mtime_nanos: 0,
            target: None,
        })
        .unwrap();
    }

    #[test]
    fn path_for_hunk() {
        assert_eq!(super::hunk_relpath(0), "00000/000000000");
    }

    #[test]
    fn basic() {
        let (testdir, mut ib) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/apple");
        add_an_entry(&mut ib, "/banana");
        ib.finish_hunk().unwrap();
        drop(ib);

        assert!(
            std::fs::metadata(testdir.path().join("00000").join("000000000"))
                .unwrap()
                .is_file(),
            "Index hunk file not found"
        );

        let mut it = IndexRead::open_path(&testdir.path())
            .iter_entries()
            .unwrap();
        let entry = it.next().expect("Get first entry");
        assert_eq!(&entry.apath, "/apple");
        let entry = it.next().expect("Get second entry");
        assert_eq!(&entry.apath, "/banana");
        assert!(it.next().is_none(), "Expected no more entries");
    }

    #[test]
    fn multiple_hunks() {
        let (testdir, mut ib) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/1.1");
        add_an_entry(&mut ib, "/1.2");
        ib.finish_hunk().unwrap();

        add_an_entry(&mut ib, "/2.1");
        add_an_entry(&mut ib, "/2.2");
        ib.finish_hunk().unwrap();

        let it = IndexRead::open_path(&testdir.path())
            .iter_entries()
            .unwrap();
        let names: Vec<String> = it.map(|x| x.apath.into()).collect();
        assert_eq!(names, &["/1.1", "/1.2", "/2.1", "/2.2"]);
    }

    #[test]
    #[should_panic]
    fn no_duplicate_paths() {
        let (_testdir, mut ib) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/hello");
        add_an_entry(&mut ib, "/hello");
    }

    #[test]
    #[should_panic]
    fn no_duplicate_paths_across_hunks() {
        let (_testdir, mut ib) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/hello");
        ib.finish_hunk().unwrap();

        // Try to add an identically-named file within the next hunk and it should error,
        // because the IndexBuilder remembers the last file name written.
        add_an_entry(&mut ib, "hello");
    }

    #[test]
    fn excluded_entries() {
        let (testdir, mut ib) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/bar");
        add_an_entry(&mut ib, "/foo");
        add_an_entry(&mut ib, "/foobar");
        ib.finish_hunk().unwrap();

        let excludes = excludes::from_strings(&["/fo*"]).unwrap();
        let it = IndexRead::open_path(&testdir.path())
            .iter_entries()
            .unwrap()
            .with_excludes(excludes);

        let names: Vec<String> = it.map(|x| x.apath.into()).collect();
        assert_eq!(names, &["/bar"]);
    }

    #[test]
    fn advance() {
        let (testdir, mut ib) = scratch_indexbuilder();
        add_an_entry(&mut ib, "/bar");
        add_an_entry(&mut ib, "/foo");
        add_an_entry(&mut ib, "/foobar");
        ib.finish_hunk().unwrap();

        // Make multiple hunks to test traversal across hunks.
        add_an_entry(&mut ib, "/g01");
        add_an_entry(&mut ib, "/g02");
        add_an_entry(&mut ib, "/g03");
        ib.finish_hunk().unwrap();

        // Advance to /foo and read on from there.
        let mut it = IndexRead::open_path(&testdir.path())
            .iter_entries()
            .unwrap();
        assert_eq!(it.advance_to(&Apath::from("/foo")).unwrap().apath, "/foo");
        assert_eq!(it.next().unwrap().apath, "/foobar");
        assert_eq!(it.next().unwrap().apath, "/g01");

        // Advance to before /g01
        let mut it = IndexRead::open_path(&testdir.path())
            .iter_entries()
            .unwrap();
        assert_eq!(it.advance_to(&Apath::from("/fxxx")), None);
        assert_eq!(it.next().unwrap().apath, "/g01");
        assert_eq!(it.next().unwrap().apath, "/g02");

        // Advance to before the first entry
        let mut it = IndexRead::open_path(&testdir.path())
            .iter_entries()
            .unwrap();
        assert_eq!(it.advance_to(&Apath::from("/aaaa")), None);
        assert_eq!(it.next().unwrap().apath, "/bar");
        assert_eq!(it.next().unwrap().apath, "/foo");

        // Advance to after the last entry
        let mut it = IndexRead::open_path(&testdir.path())
            .iter_entries()
            .unwrap();
        assert_eq!(it.advance_to(&Apath::from("/zz")), None);
        assert_eq!(it.next(), None);
    }

    /// Exactly fill the first hunk: there shouldn't be an empty second hunk.
    ///
    /// https://github.com/sourcefrog/conserve/issues/95
    #[test]
    fn no_final_empty_hunk() -> Result<()> {
        let (testdir, mut ib) = scratch_indexbuilder();
        for i in 0..MAX_ENTRIES_PER_HUNK {
            add_an_entry(&mut ib, &format!("/{:0>10}", i));
        }
        ib.finish_hunk()?;
        // Think about, but don't actually add some files
        ib.finish_hunk()?;
        let read_index = IndexRead::open_path(&testdir.path());
        assert_eq!(read_index.count_hunks()?, 1);
        Ok(())
    }
}
