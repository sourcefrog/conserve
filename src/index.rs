// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2022 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Index lists the files in a band in the archive.

use std::cmp::Ordering;
use std::io;
use std::iter::Peekable;
use std::path::Path;
use std::sync::Arc;
use std::vec;
use tracing::error;

use crate::compress::snappy::{Compressor, Decompressor};
use crate::kind::Kind;
use crate::owner::Owner;
use crate::stats::{IndexReadStats, IndexWriterStats};
use crate::transport::local::LocalTransport;
use crate::transport::Transport;
use crate::unix_mode::UnixMode;
use crate::unix_time::UnixTime;
use crate::*;

pub const MAX_ENTRIES_PER_HUNK: usize = 1000;

pub const HUNKS_PER_SUBDIR: u32 = 10_000;

/// Description of one archived file.
///
/// This struct is directly encoded/decoded to the json index file, and also can be constructed by
/// stat-ing (but not reading) a live file.
// GRCOV_EXCLUDE_START
#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IndexEntry {
    /// Path of this entry relative to the base of the backup, in `apath` form.
    pub apath: Apath,

    /// Type of file.
    pub kind: Kind,

    /// File modification time, in whole seconds past the Unix epoch.
    #[serde(default)]
    pub mtime: i64,

    /// Discretionary Access Control permissions (such as read/write/execute on unix)
    #[serde(default)]
    pub unix_mode: UnixMode,

    /// User and Group names of the owners of the file
    #[serde(default, flatten, skip_serializing_if = "Owner::is_none")]
    pub owner: Owner,

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
// GRCOV_EXCLUDE_STOP

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

    fn unix_mode(&self) -> UnixMode {
        self.unix_mode
    }

    fn owner(&self) -> Owner {
        self.owner.clone()
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
            unix_mode: source.unix_mode(),
            owner: source.owner(),
        }
    }
}

/// Write out index hunks.
///
/// This class is responsible for: remembering the hunk number, and checking that the
/// hunks preserve apath order.
pub struct IndexWriter {
    /// The `i` directory within the band where all files for this index are written.
    transport: Arc<dyn Transport>,

    /// Currently queued entries to be written out, in arbitrary order.
    entries: Vec<IndexEntry>,

    /// Index hunk number, starting at 0.
    sequence: u32,

    /// The last filename from the previous hunk, to enforce ordering. At the
    /// start of the first hunk this is empty; at the start of a later hunk it's
    /// the last path from the previous hunk.
    check_order: apath::DebugCheckOrder,

    /// Statistics about work done while writing this index.
    pub stats: IndexWriterStats,

    compressor: Compressor,
}

/// Accumulate and write out index entries into files in an index directory.
impl IndexWriter {
    /// Make a new builder that will write files into the given directory.
    pub fn new(transport: Box<dyn Transport>) -> IndexWriter {
        IndexWriter {
            transport: Arc::from(transport),
            entries: Vec::<IndexEntry>::with_capacity(MAX_ENTRIES_PER_HUNK),
            sequence: 0,
            check_order: apath::DebugCheckOrder::new(),
            stats: IndexWriterStats::default(),
            compressor: Compressor::new(),
        }
    }

    /// Finish the last hunk of this index, and return the stats.
    pub fn finish(mut self) -> Result<IndexWriterStats> {
        self.finish_hunk()?;
        Ok(self.stats)
    }

    /// Write new index entries.
    ///
    /// Entries within one hunk may be added in arbitrary order, but they must all
    /// sort after previously-written content.
    ///
    /// The new entry must sort after everything already written to the index.
    pub(crate) fn push_entry(&mut self, entry: IndexEntry) {
        self.entries.push(entry);
    }

    pub(crate) fn append_entries(&mut self, entries: &mut Vec<IndexEntry>) {
        self.entries.append(entries);
    }

    /// Finish this hunk of the index.
    ///
    /// This writes all the currently queued entries into a new index file
    /// in the band directory, and then clears the buffer to start receiving
    /// entries for the next hunk.
    pub fn finish_hunk(&mut self) -> Result<()> {
        if self.entries.is_empty() {
            return Ok(());
        }
        self.entries.sort_unstable_by(|a, b| {
            debug_assert!(a.apath != b.apath);
            a.apath.cmp(&b.apath)
        });
        self.check_order.check(&self.entries[0].apath);
        if self.entries.len() > 1 {
            self.check_order.check(&self.entries.last().unwrap().apath);
        }
        let relpath = hunk_relpath(self.sequence);
        let write_error = |source| Error::WriteIndex {
            path: relpath.clone(),
            source,
        };
        let json =
            serde_json::to_vec(&self.entries).map_err(|source| Error::SerializeIndex { source })?;
        if (self.sequence % HUNKS_PER_SUBDIR) == 0 {
            self.transport
                .create_dir(&subdir_relpath(self.sequence))
                .map_err(write_error)?;
        }
        let compressed_bytes = self.compressor.compress(&json)?;
        self.transport
            .write_file(&relpath, compressed_bytes)
            .map_err(write_error)?;

        self.stats.index_hunks += 1;
        self.stats.compressed_index_bytes += compressed_bytes.len() as u64;
        self.stats.uncompressed_index_bytes += json.len() as u64;
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
#[mutants::skip] // By default it returns "" which causes a loop. TODO: Avoid the loop.
fn hunk_relpath(hunk_number: u32) -> String {
    format!("{:05}/{:09}", hunk_number / HUNKS_PER_SUBDIR, hunk_number)
}

#[derive(Debug, Clone)]
pub struct IndexRead {
    /// Transport pointing to this index directory.
    transport: Arc<dyn Transport>,
}

impl IndexRead {
    #[allow(unused)]
    pub(crate) fn open_path(path: &Path) -> IndexRead {
        IndexRead::open(Box::new(LocalTransport::new(path)))
    }

    pub(crate) fn open(transport: Box<dyn Transport>) -> IndexRead {
        IndexRead {
            transport: Arc::from(transport),
        }
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
                .is_file(&path)
                .map_err(|source| Error::ReadIndex { source, path })?
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
    pub fn iter_entries(self) -> IndexEntryIter<IndexHunkIter> {
        // TODO: An option to pass in a subtree?
        IndexEntryIter::new(self.iter_hunks(), Apath::root(), Exclude::nothing())
    }

    /// Make an iterator that returns hunks of entries from this index.
    pub fn iter_hunks(&self) -> IndexHunkIter {
        IndexHunkIter {
            next_hunk_number: 0,
            transport: Arc::clone(&self.transport),
            decompressor: Decompressor::new(),
            stats: IndexReadStats::default(),
            after: None,
        }
    }
}

/// Read hunks of entries from a stored index, in apath order.
///
/// Each returned item is a vec of (typically up to a thousand) index entries.
pub struct IndexHunkIter {
    next_hunk_number: u32,
    /// The `i` directory within the band where all files for this index are written.
    transport: Arc<dyn Transport>,
    decompressor: Decompressor,
    pub stats: IndexReadStats,
    /// If set, yield only entries ordered after this apath.
    after: Option<Apath>,
}

impl Iterator for IndexHunkIter {
    type Item = Vec<IndexEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let hunk_number = self.next_hunk_number;
            let entries = match self.read_next_hunk() {
                Ok(None) => return None,
                Ok(Some(entries)) => entries,
                Err(err) => {
                    self.stats.errors += 1;
                    error!("Error reading index hunk {:?}: {:?} ", hunk_number, err);
                    continue;
                }
            };
            if let Some(ref after) = self.after {
                if let Some(last) = entries.last() {
                    if last.apath <= *after {
                        continue;
                    }
                }
                if let Some(first) = entries.first() {
                    if first.apath > *after {
                        self.after = None; // don't need to look again
                        return Some(entries);
                    }
                }
                let idx = match entries.binary_search_by_key(&after, |entry| &entry.apath) {
                    Ok(idx) => idx + 1, // after the point it was found
                    Err(idx) => idx,    // from the point it would have been
                };
                return Some(Vec::from(&entries[idx..]));
            }
            if !entries.is_empty() {
                return Some(entries);
            }
        }
    }
}

impl IndexHunkIter {
    /// Advance self so that it returns only entries with apaths ordered after `apath`.
    #[must_use]
    pub fn advance_to_after(self, apath: &Apath) -> Self {
        IndexHunkIter {
            after: Some(apath.clone()),
            ..self
        }
    }

    fn read_next_hunk(&mut self) -> Result<Option<Vec<IndexEntry>>> {
        let path = &hunk_relpath(self.next_hunk_number);
        // Whether we succeed or fail, don't try to read this hunk again.
        self.next_hunk_number += 1;
        let compressed_bytes = match self.transport.read_file(path) {
            Ok(b) => b,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // TODO: Cope with one hunk being missing, while there are still
                // later-numbered hunks. This would require reading the whole
                // list of hunks first.
                return Ok(None);
            }
            Err(source) => {
                return Err(Error::ReadIndex {
                    path: path.clone(),
                    source,
                });
            }
        };
        self.stats.index_hunks += 1;
        self.stats.compressed_index_bytes += compressed_bytes.len() as u64;
        let index_bytes = self.decompressor.decompress(&compressed_bytes)?;
        self.stats.uncompressed_index_bytes += index_bytes.len() as u64;
        let entries: Vec<IndexEntry> =
            serde_json::from_slice(index_bytes).map_err(|source| Error::DeserializeIndex {
                path: path.clone(),
                source,
            })?;
        if entries.is_empty() {
            // It's legal, it's just weird - and it can be produced by some old Conserve versions.
        }
        Ok(Some(entries))
    }
}

/// Read out all the entries from a stored index, in apath order.
pub struct IndexEntryIter<HI: Iterator<Item = Vec<IndexEntry>>> {
    /// Temporarily buffered entries, read from the index files but not yet
    /// returned to the client.
    buffered_entries: Peekable<vec::IntoIter<IndexEntry>>,
    hunk_iter: HI,
    subtree: Apath,
    exclude: Exclude,
}

impl<HI: Iterator<Item = Vec<IndexEntry>>> IndexEntryIter<HI> {
    pub(crate) fn new(hunk_iter: HI, subtree: Apath, exclude: Exclude) -> Self {
        IndexEntryIter {
            buffered_entries: Vec::<IndexEntry>::new().into_iter().peekable(),
            hunk_iter,
            subtree,
            exclude,
        }
    }
}

impl<HI: Iterator<Item = Vec<IndexEntry>>> Iterator for IndexEntryIter<HI> {
    type Item = IndexEntry;

    fn next(&mut self) -> Option<IndexEntry> {
        loop {
            if let Some(entry) = self.buffered_entries.next() {
                // TODO: We could be smarter about skipping ahead if nothing
                // in this page matches; or terminating early if we know
                // nothing else in the index can be under this subtree.
                if !self.subtree.is_prefix_of(&entry.apath) {
                    continue;
                }
                if self.exclude.matches(&entry.apath) {
                    continue;
                }
                return Some(entry);
            }
            if !self.refill_entry_buffer_or_warn() {
                return None;
            }
        }
    }
}

impl<HI: Iterator<Item = Vec<IndexEntry>>> IndexEntryIter<HI> {
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

    /// Read another hunk file and put it into buffered_entries.
    ///
    /// Returns true if another hunk could be found, otherwise false.
    fn refill_entry_buffer_or_warn(&mut self) -> bool {
        assert!(
            self.buffered_entries.next().is_none(),
            "refill_entry_buffer called with non-empty buffer"
        );
        if let Some(new_entries) = self.hunk_iter.next() {
            self.buffered_entries = new_entries.into_iter().peekable();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::transport::local::LocalTransport;
    use super::*;

    fn setup() -> (TempDir, IndexWriter) {
        let testdir = TempDir::new().unwrap();
        let ib = IndexWriter::new(Box::new(LocalTransport::new(testdir.path())));
        (testdir, ib)
    }

    fn sample_entry(apath: &str) -> IndexEntry {
        IndexEntry {
            apath: apath.into(),
            mtime: 1_461_736_377,
            mtime_nanos: 0,
            kind: Kind::File,
            addrs: vec![],
            target: None,
            unix_mode: Default::default(),
            owner: Default::default(),
        }
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
            unix_mode: Default::default(),
            owner: Default::default(),
        }];
        let index_json = serde_json::to_string(&entries).unwrap();
        println!("{index_json}");
        assert_eq!(
            index_json,
            "[{\"apath\":\"/a/b\",\
             \"kind\":\"File\",\
             \"mtime\":1461736377,\
             \"unix_mode\":null}]"
        );
    }

    #[test]
    fn index_builder_sorts_entries() {
        let (_testdir, mut ib) = setup();
        ib.push_entry(sample_entry("/zzz"));
        ib.push_entry(sample_entry("/aaa"));
        ib.finish_hunk().unwrap();
    }

    #[test]
    #[should_panic]
    fn index_builder_checks_names() {
        let (_testdir, mut ib) = setup();
        ib.push_entry(sample_entry("../escapecat"));
        ib.finish_hunk().unwrap();
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn no_duplicate_paths() {
        let (_testdir, mut ib) = setup();
        ib.push_entry(sample_entry("/again"));
        ib.push_entry(sample_entry("/again"));
        ib.finish_hunk().unwrap();
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn no_duplicate_paths_across_hunks() {
        let (_testdir, mut ib) = setup();
        ib.push_entry(sample_entry("/again"));
        ib.finish_hunk().unwrap();
        ib.push_entry(sample_entry("/again"));
        ib.finish_hunk().unwrap();
    }

    #[test]
    fn path_for_hunk() {
        assert_eq!(super::hunk_relpath(0), "00000/000000000");
    }

    #[test]
    fn basic() {
        let (testdir, mut ib) = setup();
        ib.append_entries(&mut vec![sample_entry("/apple"), sample_entry("/banana")]);
        let stats = ib.finish().unwrap();

        assert_eq!(stats.index_hunks, 1);
        assert!(stats.compressed_index_bytes > 30);
        assert!(
            stats.compressed_index_bytes <= 125,
            "expected shorter compressed index: {}",
            stats.compressed_index_bytes
        );
        assert!(stats.uncompressed_index_bytes > 100);
        assert!(
            stats.uncompressed_index_bytes < 250,
            "expected shorter uncompressed index: {}",
            stats.uncompressed_index_bytes
        );

        assert!(
            std::fs::metadata(testdir.path().join("00000").join("000000000"))
                .unwrap()
                .is_file(),
            "Index hunk file not found"
        );

        let mut it = IndexRead::open_path(testdir.path()).iter_entries();
        let entry = it.next().expect("Get first entry");
        assert_eq!(&entry.apath, "/apple");
        let entry = it.next().expect("Get second entry");
        assert_eq!(&entry.apath, "/banana");
        assert!(it.next().is_none(), "Expected no more entries");
    }

    #[test]
    fn multiple_hunks() {
        let (testdir, mut ib) = setup();
        ib.append_entries(&mut vec![sample_entry("/1.1"), sample_entry("/1.2")]);
        ib.finish_hunk().unwrap();
        ib.append_entries(&mut vec![sample_entry("/2.1"), sample_entry("/2.2")]);
        ib.finish_hunk().unwrap();

        let index_read = IndexRead::open_path(testdir.path());
        let it = index_read.iter_entries();
        let names: Vec<String> = it.map(|x| x.apath.into()).collect();
        assert_eq!(names, &["/1.1", "/1.2", "/2.1", "/2.2"]);

        // Read it out as hunks.
        let hunks: Vec<Vec<IndexEntry>> =
            IndexRead::open_path(testdir.path()).iter_hunks().collect();
        assert_eq!(hunks.len(), 2);
        assert_eq!(
            hunks[0]
                .iter()
                .map(|entry| entry.apath())
                .collect::<Vec<_>>(),
            vec!["/1.1", "/1.2"]
        );
        assert_eq!(
            hunks[1]
                .iter()
                .map(|entry| entry.apath())
                .collect::<Vec<_>>(),
            vec!["/2.1", "/2.2"]
        );
    }

    #[test]
    fn iter_hunks_advance_to_after() {
        let (testdir, mut ib) = setup();
        ib.append_entries(&mut vec![sample_entry("/1.1"), sample_entry("/1.2")]);
        ib.finish_hunk().unwrap();
        ib.append_entries(&mut vec![sample_entry("/2.1"), sample_entry("/2.2")]);
        ib.finish_hunk().unwrap();

        let index_read = IndexRead::open_path(testdir.path());
        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/1.1", "/1.2", "/2.1", "/2.2"]);

        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/nonexistent".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, [""; 0]);

        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/1.1".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/1.2", "/2.1", "/2.2"]);

        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/1.1.1".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/1.2", "/2.1", "/2.2"]);

        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/1.2".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/2.1", "/2.2"]);

        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/1.3".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/2.1", "/2.2"]);

        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/2.0".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/2.1", "/2.2"]);

        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/2.1".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/2.2"]);

        let names: Vec<String> = index_read
            .iter_hunks()
            .advance_to_after(&"/2.2".into())
            .flatten()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, [] as [&str; 0]);
    }

    #[test]
    fn advance() {
        let (testdir, mut ib) = setup();
        ib.push_entry(sample_entry("/bar"));
        ib.push_entry(sample_entry("/foo"));
        ib.push_entry(sample_entry("/foobar"));
        ib.finish_hunk().unwrap();

        // Make multiple hunks to test traversal across hunks.
        ib.push_entry(sample_entry("/g01"));
        ib.push_entry(sample_entry("/g02"));
        ib.push_entry(sample_entry("/g03"));
        ib.finish_hunk().unwrap();

        // Advance to /foo and read on from there.
        let mut it = IndexRead::open_path(testdir.path()).iter_entries();
        assert_eq!(it.advance_to(&Apath::from("/foo")).unwrap().apath, "/foo");
        assert_eq!(it.next().unwrap().apath, "/foobar");
        assert_eq!(it.next().unwrap().apath, "/g01");

        // Advance to before /g01
        let mut it = IndexRead::open_path(testdir.path()).iter_entries();
        assert_eq!(it.advance_to(&Apath::from("/fxxx")), None);
        assert_eq!(it.next().unwrap().apath, "/g01");
        assert_eq!(it.next().unwrap().apath, "/g02");

        // Advance to before the first entry
        let mut it = IndexRead::open_path(testdir.path()).iter_entries();
        assert_eq!(it.advance_to(&Apath::from("/aaaa")), None);
        assert_eq!(it.next().unwrap().apath, "/bar");
        assert_eq!(it.next().unwrap().apath, "/foo");

        // Advance to after the last entry
        let mut it = IndexRead::open_path(testdir.path()).iter_entries();
        assert_eq!(it.advance_to(&Apath::from("/zz")), None);
        assert_eq!(it.next(), None);
    }

    /// Exactly fill the first hunk: there shouldn't be an empty second hunk.
    ///
    /// https://github.com/sourcefrog/conserve/issues/95
    #[test]
    fn no_final_empty_hunk() -> Result<()> {
        let (testdir, mut ib) = setup();
        for i in 0..MAX_ENTRIES_PER_HUNK {
            ib.push_entry(sample_entry(&format!("/{i:0>10}")));
        }
        ib.finish_hunk()?;
        // Think about, but don't actually add some files
        ib.finish_hunk()?;
        let read_index = IndexRead::open_path(testdir.path());
        assert_eq!(read_index.count_hunks()?, 1);
        Ok(())
    }
}
