// Conserve backup system.
// Copyright 2015-2025 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! The index lists all the files in a backup, sorted in apath order.

use itertools::Itertools;
use tracing::{debug, debug_span, error};

use crate::compress::snappy::Decompressor;
use crate::stats::IndexReadStats;
use crate::transport::Transport;
use crate::*;

pub mod stitch;

pub(crate) mod entry;
use self::entry::IndexEntry;

mod write;
pub use write::IndexWriter;

pub const HUNKS_PER_SUBDIR: u32 = 10_000;

/// Return the transport-relative path for a subdirectory.
fn subdir_relpath(hunk_number: u32) -> String {
    format!("{:05}", hunk_number / HUNKS_PER_SUBDIR)
}

/// Return the relative path for a hunk.
#[mutants::skip] // By default it returns "" which causes a loop. TODO: Avoid the loop.
fn hunk_relpath(hunk_number: u32) -> String {
    format!("{:05}/{:09}", hunk_number / HUNKS_PER_SUBDIR, hunk_number)
}

/// Utility to read the stored index
pub struct IndexRead {
    /// Transport pointing to this index directory.
    transport: Transport,

    /// Decompressor for the index to read
    decompressor: Decompressor,

    /// Current read statistics of this index
    pub stats: IndexReadStats,
}

impl IndexRead {
    #[cfg(test)]
    pub(crate) fn open_path(path: &std::path::Path) -> IndexRead {
        IndexRead::open(Transport::local(path))
    }

    pub(crate) fn open(transport: Transport) -> IndexRead {
        IndexRead {
            transport,
            decompressor: Decompressor::new(),
            stats: IndexReadStats::default(),
        }
    }

    /// Clone the read index.
    /// Note:
    /// This has several side effects:
    /// - Depending on the implementation of the decompressor, duplicate might not be a cheap option.
    /// - Every read index has its own unique read stats, therefore the clone does not inherit the read stats.
    pub(crate) fn duplicate(&self) -> Self {
        Self::open(self.transport.clone())
    }

    /// Read and parse a specific hunk
    pub async fn read_hunk(&mut self, hunk_number: u32) -> Result<Option<Vec<IndexEntry>>> {
        let path = hunk_relpath(hunk_number);
        let compressed_bytes = match self.transport.read(&path).await {
            Ok(b) => b,
            Err(err) if err.is_not_found() => {
                // TODO: Cope with one hunk being missing, while there are still
                // later-numbered hunks. This would require reading the whole
                // list of hunks first.
                return Ok(None);
            }
            Err(source) => {
                self.stats.errors += 1;
                error!("Error reading index hunk {hunk_number:?}: {source}");
                return Err(Error::Transport { source });
            }
        };
        self.stats.index_hunks += 1;
        self.stats.compressed_index_bytes += compressed_bytes.len() as u64;
        let index_bytes = self.decompressor.decompress(&compressed_bytes)?;
        self.stats.uncompressed_index_bytes += index_bytes.len() as u64;
        let entries: Vec<IndexEntry> =
            serde_json::from_slice(&index_bytes).map_err(|source| Error::DeserializeJson {
                path: path.clone(),
                source,
            })?;
        if entries.is_empty() {
            // It's legal, it's just weird - and it can be produced by some old Conserve versions.
        }
        Ok(Some(entries))
    }

    // All hunk numbers present in all directories.
    pub async fn hunks_available(&self) -> Result<Vec<u32>> {
        let subdirs = self.transport.list_dir("").await?.dirs.into_iter().sorted();
        let mut hunks = Vec::new();
        for dir in subdirs {
            if let Ok(list) = self.transport.list_dir(&dir).await {
                hunks.extend(
                    list.files
                        .iter()
                        .filter_map(|f| f.parse::<u32>().ok())
                        .sorted(),
                )
            }
        }
        Ok(hunks)
    }

    /// Make an iterator that returns hunks of entries from this index,
    /// skipping any that are not present.
    pub async fn iter_available_hunks(self) -> IndexHunkIter {
        let _span = debug_span!("iter_hunks", ?self.transport).entered();
        let hunks = self.hunks_available().await.expect("hunks available"); // TODO: Don't panic
        debug!(?hunks);
        IndexHunkIter {
            hunks: hunks.into_iter(),
            index: self,
            after: None,
        }
    }
}

/// Read hunks of entries from a stored index, in apath order.
///
/// Each returned item is a vec of (typically up to a thousand) index entries.
pub struct IndexHunkIter {
    hunks: std::vec::IntoIter<u32>,
    pub index: IndexRead,
    /// If set, yield only entries ordered after this apath.
    after: Option<Apath>,
}

impl IndexHunkIter {
    // TODO: Maybe this should return Results so that errors can be
    // more easily observed.

    pub async fn next(&mut self) -> Option<Vec<IndexEntry>> {
        loop {
            let hunk_number = self.hunks.next()?;
            let entries = match self.index.read_hunk(hunk_number).await {
                Ok(None) => return None,
                Ok(Some(entries)) => entries,
                Err(_err) => {
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

    /// Collect the contents of the iterator into a vector of hunks, each of which
    /// contains vector of entries.
    ///
    /// This reads the whole index into memory so is not recommended for large trees.
    pub async fn collect_hunk_vec(&mut self) -> Result<Vec<Vec<IndexEntry>>> {
        let mut hunks = Vec::new();
        while let Some(hunk) = self.next().await {
            hunks.push(hunk);
        }
        Ok(hunks)
    }

    /// Collect the contents of the index into a vec of entries.
    ///
    /// This is the flattened version of `collect_hunk_vec`.
    ///
    /// This reads the whole index into memory so is not recommended for large trees.
    pub async fn collect_entry_vec(&mut self) -> Result<Vec<IndexEntry>> {
        let mut entries = Vec::new();
        while let Some(hunk) = self.next().await {
            entries.extend(hunk);
        }
        Ok(entries)
    }

    /// Advance self so that it returns only entries with apaths ordered after `apath`.
    #[must_use]
    pub fn advance_to_after(self, apath: &Apath) -> Self {
        IndexHunkIter {
            after: Some(apath.clone()),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::{counters::Counter, monitor::test::TestMonitor};

    use super::*;

    fn setup() -> (TempDir, IndexWriter) {
        let testdir = TempDir::new().unwrap();
        let ib = IndexWriter::new(Transport::local(testdir.path()), TestMonitor::arc());
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

    #[tokio::test]
    async fn basic() -> Result<()> {
        let transport = Transport::temp();
        let monitor = TestMonitor::arc();
        let mut index_writer = IndexWriter::new(transport.clone(), monitor.clone());
        index_writer.append_entries(&mut vec![sample_entry("/apple"), sample_entry("/banana")]);
        let hunks = index_writer.finish().unwrap();
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 1);

        assert_eq!(hunks, 1);
        let counters = monitor.counters();
        dbg!(&counters);
        assert!(counters.get(Counter::IndexWriteCompressedBytes) > 30);
        assert!(counters.get(Counter::IndexWriteCompressedBytes) < 125,);
        assert!(counters.get(Counter::IndexWriteUncompressedBytes) > 100);
        assert!(counters.get(Counter::IndexWriteUncompressedBytes) < 250);

        assert!(
            transport.is_file("00000/000000000").await.unwrap(),
            "Index hunk file not found"
        );

        let hunks = IndexRead::open(transport.clone())
            .iter_available_hunks()
            .await
            .collect_hunk_vec()
            .await?;
        assert_eq!(hunks.len(), 1);
        let entries = &hunks[0];
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].apath, "/apple");
        assert_eq!(entries[1].apath, "/banana");
        Ok(())
    }

    #[tokio::test]
    async fn multiple_hunks() -> Result<()> {
        let (testdir, mut ib) = setup();
        ib.append_entries(&mut vec![sample_entry("/1.1"), sample_entry("/1.2")]);
        ib.finish_hunk().unwrap();
        ib.append_entries(&mut vec![sample_entry("/2.1"), sample_entry("/2.2")]);
        ib.finish_hunk().unwrap();

        let index_read = IndexRead::open_path(testdir.path());
        let names = index_read
            .iter_available_hunks()
            .await
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|e| e.apath.to_string())
            .collect_vec();
        assert_eq!(names, &["/1.1", "/1.2", "/2.1", "/2.2"]);

        // Read it out as hunks.
        let hunks: Vec<Vec<IndexEntry>> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .collect_hunk_vec()
            .await?;
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
        Ok(())
    }

    #[tokio::test]
    async fn iter_hunks_advance_to_after() -> Result<()> {
        let (testdir, mut ib) = setup();
        ib.append_entries(&mut vec![sample_entry("/1.1"), sample_entry("/1.2")]);
        ib.finish_hunk().unwrap();
        ib.append_entries(&mut vec![sample_entry("/2.1"), sample_entry("/2.2")]);
        ib.finish_hunk().unwrap();

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/1.1", "/1.2", "/2.1", "/2.2"]);

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/nonexistent".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, [""; 0]);

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/1.1".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/1.2", "/2.1", "/2.2"]);

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/1.1.1".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/1.2", "/2.1", "/2.2"]);

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/1.2".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/2.1", "/2.2"]);

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/1.3".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/2.1", "/2.2"]);

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/2.0".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/2.1", "/2.2"]);

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/2.1".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, ["/2.2"]);

        let names: Vec<String> = IndexRead::open_path(testdir.path())
            .iter_available_hunks()
            .await
            .advance_to_after(&"/2.2".into())
            .collect_entry_vec()
            .await?
            .into_iter()
            .map(|entry| entry.apath.into())
            .collect();
        assert_eq!(names, [] as [&str; 0]);
        Ok(())
    }

    /// Exactly fill the first hunk: there shouldn't be an empty second hunk.
    ///
    /// https://github.com/sourcefrog/conserve/issues/95
    #[tokio::test]
    async fn no_final_empty_hunk() -> Result<()> {
        let (testdir, mut ib) = setup();
        for i in 0..1000 {
            ib.push_entry(sample_entry(&format!("/{i:0>10}")));
        }
        ib.finish_hunk()?;
        // Think about, but don't actually add some files
        ib.finish_hunk()?;
        dbg!(ib.hunks_written);
        let read_index = IndexRead::open_path(testdir.path());
        let hunks = read_index
            .iter_available_hunks()
            .await
            .collect_hunk_vec()
            .await?;
        assert_eq!(hunks.len(), 1);
        Ok(())
    }
}
