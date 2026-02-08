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

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::fmt;
use std::io::prelude::*;
use std::mem::take;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

use bytes::BytesMut;
use derive_more::{Add, AddAssign};
use tracing::{trace, warn};

use crate::blockdir::{Address, BlockDir};
use crate::change::Change;
use crate::counters::Counter;
use crate::index::entry::IndexEntry;
use crate::index::stitch::Stitch;
use crate::io::read_with_retries;
use crate::monitor::Monitor;
use crate::stats::{write_compressed_size, write_count, write_duration, write_size};
use crate::*;

/// Configuration of how to make a backup.
pub struct BackupOptions {
    /// Exclude these globs from the backup.
    pub exclude: Exclude,

    /// Maximum number of index entries per index hunk.
    pub max_entries_per_hunk: usize,

    /// Call this callback as each entry is successfully stored.
    pub change_callback: Option<ChangeCallback>,

    pub max_block_size: usize,

    /// Combine files smaller than this into a single block.
    pub small_file_cap: u64,

    /// Record the user/group owners on Unix.
    pub owner: bool,
}

impl Default for BackupOptions {
    fn default() -> BackupOptions {
        BackupOptions {
            exclude: Exclude::nothing(),
            max_entries_per_hunk: 100_000,
            change_callback: None,
            max_block_size: 20 << 20,
            small_file_cap: 1 << 20,
            owner: true,
        }
    }
}

// This causes us to walk the source tree twice, which is probably an acceptable option
// since it's nice to see realistic overall progress. We could keep all the entries
// in memory, and maybe we should, but it might get unreasonably big.
// if options.measure_first {
//     progress_bar.set_phase("Measure source tree".to_owned());
//     // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
//     // again a second time? But, that'll potentially use memory proportional to tree size, which
//     // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
//     // deleted or changed while this is running.
//     progress_bar.set_bytes_total(source.size()?.file_bytes as u64);
// }

/// Backup a source directory into a new band in the archive.
///
/// Returns statistics about what was copied.
pub async fn backup(
    archive: &Archive,
    source_path: &Path,
    options: &BackupOptions,
    monitor: Arc<dyn Monitor>,
) -> Result<BackupStats> {
    let start = Instant::now();
    if gc_lock::GarbageCollectionLock::is_locked(archive).await? {
        return Err(Error::GarbageCollectionLockHeld);
    }
    let source_tree = SourceTree::open(source_path)?;
    let mut stats = BackupStats::default();
    let task = monitor.start_task("Backup".to_string());
    let basis_index = if let Some(basis_band_id) = archive.last_band_id().await? {
        Stitch::new(
            archive,
            basis_band_id,
            Apath::root(),
            Exclude::nothing(),
            monitor.clone(),
        )
    } else {
        Stitch::empty(archive, monitor.clone())
    };

    let source_entries =
        source_tree.iter_entries(Apath::root(), options.exclude.clone(), monitor.clone())?;
    let mut merge = MergeTrees::new(basis_index, source_entries);

    // Create the new band only after finding the basis band!
    let band = Band::create(archive).await?;
    let index_writer = band.index_writer(monitor.clone());
    let mut writer = BackupWriter {
        band,
        index_writer,
        block_dir: archive.block_dir.clone(),
        stats: BackupStats::default(),
        file_combiner: FileCombiner::new(archive.block_dir.clone(), options.max_block_size),
    };

    while let Some(merged_entries) = merge.next().await {
        trace!(?merged_entries);
        let (basis_entry, source_entry) = merged_entries.into_options();
        if let Some(source_entry) = source_entry {
            trace!(apath = %source_entry.apath(), has_basis = basis_entry.is_some(), "Copying");
            task.set_name(format!("Backup {}", source_entry.apath()));
            match writer
                .copy_entry(
                    &basis_entry,
                    source_entry,
                    &source_tree,
                    options,
                    monitor.clone(),
                )
                .await
            {
                Err(err) => {
                    monitor.error(err);
                    stats.errors += 1;
                    continue;
                }
                Ok(Some(entry_change)) => {
                    match entry_change.change {
                        Change::Changed { .. } => monitor.count(Counter::EntriesChanged, 1),
                        Change::Added { .. } => monitor.count(Counter::EntriesAdded, 1),
                        Change::Unchanged { .. } => monitor.count(Counter::EntriesUnchanged, 1),
                        Change::Deleted { .. } => panic!("Deleted should not be returned here"),
                    }
                    if let Some(cb) = &options.change_callback {
                        cb(&entry_change)?;
                    }
                }
                Ok(_) => {}
            }
            trace!(
                index_queue = writer.index_writer.pending_entries(),
                combiner_queue = writer.file_combiner.queue.len(),
                "After copy"
            );
            if writer.index_writer.pending_entries() + writer.file_combiner.queue.len()
                >= options.max_entries_per_hunk
            {
                writer.flush_group(monitor.clone()).await?;
                assert_eq!(writer.index_writer.pending_entries(), 0);
            }
        } else {
            // This entry was in the basis but not in the source.
            let basis_entry = basis_entry.expect("Basis entry must exist if source entry is none");
            trace!(apath = %basis_entry.apath(), "Deleted");
            monitor.count(Counter::EntriesDeleted, 1);
            options
                .change_callback
                .as_ref()
                .map(|cb| cb(&EntryChange::deleted(&basis_entry)));
        }
    }
    stats += writer.finish(monitor.clone()).await?;
    stats.elapsed = start.elapsed();
    let block_stats = &archive.block_dir.stats;
    stats.read_blocks = block_stats.read_blocks.load(Relaxed);
    stats.read_blocks_compressed_bytes = block_stats.read_block_compressed_bytes.load(Relaxed);
    stats.read_blocks_uncompressed_bytes = block_stats.read_block_uncompressed_bytes.load(Relaxed);
    // TODO: Merge in stats from the source tree?
    Ok(stats)
}

/// Accepts files to write in the archive (in apath order.)
struct BackupWriter {
    band: Band,
    index_writer: IndexWriter,
    stats: BackupStats,
    block_dir: Arc<BlockDir>,

    file_combiner: FileCombiner,
}

impl BackupWriter {
    async fn finish(mut self, monitor: Arc<dyn Monitor>) -> Result<BackupStats> {
        self.flush_group(monitor.clone()).await?;
        let hunks = self.index_writer.finish().await?;
        trace!(?hunks, "Closing band");
        self.band.close(hunks as u64).await?;
        Ok(BackupStats { ..self.stats })
    }

    /// Write out any pending data blocks, and then the pending index entries.
    async fn flush_group(&mut self, monitor: Arc<dyn Monitor>) -> Result<()> {
        let (stats, mut entries) = self.file_combiner.drain(monitor.clone()).await?;
        trace!("Got {} entries to write from file combiner", entries.len());
        self.stats += stats;
        self.index_writer.append_entries(&mut entries);
        self.index_writer.finish_hunk().await?;
        Ok(())
    }

    /// Add one entry to the backup.
    ///
    /// Return an indication of whether it changed (if it's a file), or
    /// None for non-plain-file types where that information is not currently
    /// calculated.
    async fn copy_entry(
        &mut self,
        basis_entry: &Option<IndexEntry>,
        mut source_entry: source::Entry,
        source_tree: &SourceTree,
        options: &BackupOptions,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Option<EntryChange>> {
        if !options.owner {
            source_entry.owner.clear();
        }
        // TODO: Emit deletions for entries in the basis not present in the source,
        // probably by using Merge to read both trees in parallel.
        match source_entry.kind() {
            Kind::Dir => self.copy_dir(&source_entry, monitor.as_ref()),
            Kind::File => {
                self.copy_file(
                    &source_entry,
                    source_tree,
                    basis_entry,
                    options,
                    monitor.clone(),
                )
                .await
            }
            Kind::Symlink => self.copy_symlink(&source_entry, monitor.as_ref()),
            Kind::Unknown => {
                self.stats.unknown_kind += 1;
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                Ok(None)
            }
        }
    }

    fn copy_dir(
        &mut self,
        source_entry: &source::Entry,
        monitor: &dyn Monitor,
    ) -> Result<Option<EntryChange>> {
        monitor.count(Counter::Dirs, 1);
        self.stats.directories += 1;
        self.index_writer
            .push_entry(IndexEntry::metadata_from(source_entry));
        Ok(None) // TODO: Emit the actual change.
    }

    /// Copy in the contents of a file from another tree.
    async fn copy_file(
        &mut self,
        source_entry: &source::Entry,
        source_tree: &SourceTree,
        basis_entry: &Option<IndexEntry>,
        options: &BackupOptions,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Option<EntryChange>> {
        self.stats.files += 1;
        monitor.count(Counter::Files, 1);
        let apath = source_entry.apath();
        trace!(?apath, "Copying file");
        let result = if let Some(basis_entry) = basis_entry {
            if content_heuristically_unchanged(source_entry, basis_entry) {
                if basis_entry
                    .addrs
                    .iter()
                    .all(|addr| self.block_dir.contains(&addr.hash))
                {
                    self.stats.unmodified_files += 1;
                    let new_entry = IndexEntry {
                        addrs: basis_entry.addrs.clone(),
                        ..IndexEntry::metadata_from(source_entry)
                    };
                    let change = if new_entry == *basis_entry {
                        EntryChange::unchanged(basis_entry)
                    } else {
                        trace!(%apath, "Content same, metadata changed");
                        EntryChange::changed(basis_entry, source_entry)
                    };
                    self.index_writer.push_entry(new_entry);
                    return Ok(Some(change));
                } else {
                    warn!(%apath, ?basis_entry.addrs, "Some referenced blocks are missing or truncated; file will be stored again");
                    self.stats.modified_files += 1;
                    self.stats.replaced_damaged_blocks += 1;
                    Some(EntryChange::changed(basis_entry, source_entry))
                }
            } else {
                self.stats.modified_files += 1;
                Some(EntryChange::changed(basis_entry, source_entry))
            }
        } else {
            self.stats.new_files += 1;
            trace!("New file");
            Some(EntryChange::added(source_entry))
        };
        let size = source_entry.size().expect("source entry has a size");
        if size == 0 {
            self.index_writer
                .push_entry(IndexEntry::metadata_from(source_entry));
            self.stats.empty_files += 1;
            monitor.count(Counter::EmptyFiles, 1);
        } else {
            let mut source_file = source_tree.open_file(&source_entry.apath)?;
            if size <= options.small_file_cap {
                trace!(%apath, "Combining small file");
                self.file_combiner
                    .push_file(source_entry, &mut source_file, monitor.clone())
                    .await?;
                monitor.count(Counter::SmallFiles, 1);
            } else {
                let addrs = store_file_content(
                    apath,
                    &mut source_file,
                    &self.block_dir,
                    &mut self.stats,
                    options.max_block_size,
                    monitor.clone(),
                )
                .await?;
                self.index_writer.push_entry(IndexEntry {
                    addrs,
                    ..IndexEntry::metadata_from(source_entry)
                });
            }
        }
        Ok(result)
    }

    fn copy_symlink(
        &mut self,
        source_entry: &source::Entry,
        monitor: &dyn Monitor,
    ) -> Result<Option<EntryChange>> {
        monitor.count(Counter::Symlinks, 1);
        let target = source_entry.symlink_target();
        self.stats.symlinks += 1;
        assert!(target.is_some());
        self.index_writer
            .push_entry(IndexEntry::metadata_from(source_entry));
        // TODO: Emit the actual change.
        Ok(None)
    }
}

async fn store_file_content(
    apath: &Apath,
    from_file: &mut dyn Read,
    block_dir: &BlockDir,
    stats: &mut BackupStats,
    max_block_size: usize,
    monitor: Arc<dyn Monitor>,
) -> Result<Vec<Address>> {
    let mut addresses = Vec::<Address>::with_capacity(1);
    loop {
        let buffer = read_with_retries(max_block_size, from_file).map_err(|source| {
            Error::ReadSourceFile {
                path: apath.to_string().into(),
                source,
            }
        })?;
        if buffer.is_empty() {
            break;
        }
        let buffer = buffer.freeze();
        monitor.count(Counter::FileBytes, buffer.len());
        let len = buffer.len() as u64;
        let hash = block_dir
            .store_or_deduplicate(buffer, stats, monitor.clone())
            .await?;
        addresses.push(Address {
            hash,
            start: 0,
            len,
        });
    }
    match addresses.len() {
        0 => {
            // This doesn't duplicate the call to monitor.count above, because
            // in this case we only discovered that it was empty after reading the
            // file.
            monitor.count(Counter::EmptyFiles, 1);
            stats.empty_files += 1;
        }
        1 => {
            monitor.count(Counter::SingleBlockFiles, 1);
            stats.single_block_files += 1
        }
        _ => {
            monitor.count(Counter::MultiBlockFiles, 1);
            stats.multi_block_files += 1
        }
    }
    Ok(addresses)
}

/// Combines multiple small files into a single block.
///
/// When the block is finished, and only then, this returns the index entries with the addresses
/// completed.
struct FileCombiner {
    /// Buffer of concatenated data from small files.
    buf: BytesMut,
    queue: Vec<QueuedFile>,
    /// Entries for files that have been written to the blockdir, and that have complete addresses.
    finished: Vec<IndexEntry>,
    stats: BackupStats,
    block_dir: Arc<BlockDir>,
    max_block_size: usize,
}

/// A file in the process of being written into a combined block.
///
/// While this exists, the data has been stored into the combine buffer, and we know
/// the offset and length. But since the combine buffer hasn't been finished and hashed,
/// we do not yet know a full address.
struct QueuedFile {
    /// Offset of the start of the data from this file within `buf`.
    start: usize,
    /// Length of data in this file.
    len: usize,
    /// IndexEntry without addresses.
    entry: IndexEntry,
}

impl FileCombiner {
    fn new(block_dir: Arc<BlockDir>, max_block_size: usize) -> FileCombiner {
        FileCombiner {
            block_dir,
            buf: BytesMut::new(),
            queue: Vec::new(),
            finished: Vec::new(),
            stats: BackupStats::default(),
            max_block_size,
        }
    }

    /// Flush any pending files, and return accumulated file entries and stats.
    /// The FileCombiner is then empty and ready for reuse.
    async fn drain(&mut self, monitor: Arc<dyn Monitor>) -> Result<(BackupStats, Vec<IndexEntry>)> {
        self.flush(monitor).await?;
        debug_assert!(self.queue.is_empty());
        debug_assert!(self.buf.is_empty());
        Ok((
            std::mem::take(&mut self.stats),
            std::mem::take(&mut self.finished),
        ))
    }

    /// Write all the content from the combined block to a blockdir.
    ///
    /// Returns the fully populated entries for all files in this combined block.
    ///
    /// After this call the FileCombiner is empty and can be reused for more files into a new
    /// block.
    async fn flush(&mut self, monitor: Arc<dyn Monitor>) -> Result<()> {
        if self.queue.is_empty() {
            debug_assert!(self.buf.is_empty());
            return Ok(());
        }
        let hash = self
            .block_dir
            .store_or_deduplicate(take(&mut self.buf).freeze(), &mut self.stats, monitor)
            .await?;
        self.stats.combined_blocks += 1;
        self.finished
            .extend(self.queue.drain(..).map(|qf| IndexEntry {
                addrs: vec![Address {
                    hash: hash.clone(),
                    start: qf.start.try_into().unwrap(),
                    len: qf.len.try_into().unwrap(),
                }],
                ..qf.entry
            }));
        Ok(())
    }

    /// Add the contents of a small file into this combiner.
    ///
    /// `entry` should be an IndexEntry that's complete apart from the block addresses.
    async fn push_file(
        &mut self,
        entry: &source::Entry,
        from_file: &mut dyn Read,
        monitor: Arc<dyn Monitor>,
    ) -> Result<()> {
        let start = self.buf.len();
        let expected_len: usize = entry
            .size()
            .expect("small file has no length")
            .try_into()
            .unwrap();
        let index_entry = IndexEntry::metadata_from(entry);
        if expected_len == 0 {
            self.stats.empty_files += 1;
            self.finished.push(index_entry);
            return Ok(());
        }
        self.buf.resize(start + expected_len, 0);
        let len =
            from_file
                .read(&mut self.buf[start..])
                .map_err(|source| Error::ReadSourceFile {
                    path: entry.apath.to_string().into(),
                    source,
                })?;
        self.buf.truncate(start + len);
        if len == 0 {
            self.stats.empty_files += 1;
            self.finished.push(index_entry);
            return Ok(());
        }
        // TODO: Check whether this file is exactly the same as, or a prefix of,
        // one already stored inside this block. In that case trim the buffer and
        // use the existing start/len.
        self.stats.small_combined_files += 1;
        self.queue.push(QueuedFile {
            start,
            len,
            entry: index_entry,
        });
        // TODO: This can overrun by one small file; it would be better to check
        // in advance and perhaps start a new combined block that it will fit inside.
        if self.buf.len() >= self.max_block_size {
            self.flush(monitor).await
        } else {
            Ok(())
        }
    }
}

/// True if the metadata supports an assumption the file contents have
/// not changed, without reading the file content.
///
/// Caution: this does not check the symlink target.
fn content_heuristically_unchanged<E: EntryTrait, O: EntryTrait>(
    new_entry: &E,
    basis_entry: &O,
) -> bool {
    basis_entry.kind() == new_entry.kind()
        && basis_entry.mtime() == new_entry.mtime()
        && basis_entry.size() == new_entry.size()
}

#[derive(Add, AddAssign, Debug, Default, Eq, PartialEq, Clone)]
pub struct BackupStats {
    // TODO: Include source file bytes, including unmodified files.
    pub files: usize,
    pub symlinks: usize,
    pub directories: usize,
    pub unknown_kind: usize,

    pub unmodified_files: usize,
    pub modified_files: usize,
    pub new_files: usize,

    /// Files that were previously stored and that have been stored again because
    /// some of their blocks were damaged.
    pub replaced_damaged_blocks: usize,

    /// Bytes that matched an existing block.
    pub deduplicated_bytes: u64,
    /// Bytes that were stored as new blocks, before compression.
    pub uncompressed_bytes: u64,
    pub compressed_bytes: u64,

    pub deduplicated_blocks: usize,
    pub written_blocks: usize,
    /// Blocks containing combined small files.
    pub combined_blocks: usize,

    pub empty_files: usize,
    pub small_combined_files: usize,
    pub single_block_files: usize,
    pub multi_block_files: usize,

    pub errors: usize,

    pub elapsed: Duration,

    pub read_blocks: usize,
    pub read_blocks_uncompressed_bytes: usize,
    pub read_blocks_compressed_bytes: usize,
}

impl fmt::Display for BackupStats {
    fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_count(w, "files:", self.files);
        write_count(w, "  unmodified files", self.unmodified_files);
        write_count(w, "  modified files", self.modified_files);
        write_count(w, "  new files", self.new_files);
        write_count(w, "symlinks", self.symlinks);
        write_count(w, "directories", self.directories);
        write_count(w, "unsupported file kind", self.unknown_kind);
        writeln!(w).unwrap();

        write_count(w, "files stored:", self.new_files + self.modified_files);
        write_count(w, "  empty files", self.empty_files);
        write_count(w, "  small combined files", self.small_combined_files);
        write_count(w, "  single block files", self.single_block_files);
        write_count(w, "  multi-block files", self.multi_block_files);
        writeln!(w).unwrap();

        write_count(w, "data blocks deduplicated:", self.deduplicated_blocks);
        write_size(w, "  saved", self.deduplicated_bytes);
        writeln!(w).unwrap();

        write_count(w, "new data blocks written:", self.written_blocks);
        write_count(w, "  blocks of combined files", self.combined_blocks);
        write_compressed_size(w, self.compressed_bytes, self.uncompressed_bytes);
        writeln!(w).unwrap();

        write_count(w, "blocks read", self.read_blocks);
        write_size(
            w,
            "  uncompressed",
            self.read_blocks_uncompressed_bytes as u64,
        );
        write_size(w, "  compressed", self.read_blocks_compressed_bytes as u64);
        writeln!(w).unwrap();

        write_count(w, "errors", self.errors);
        write_duration(w, "elapsed", self.elapsed)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use std::sync::Mutex;

    use assert_fs::{TempDir, prelude::*};

    use filetime::{FileTime, set_file_mtime};

    use crate::counters::Counter;
    use crate::monitor::test::TestMonitor;
    use crate::test_fixtures::TreeFixture;
    use crate::transport::Transport;
    use crate::transport::record::Verb;
    use crate::*;

    use super::*;

    #[tokio::test]
    async fn deleted_files_are_reported() {
        // tracing_subscriber::fmt::init();

        let archive = Archive::create_temp().await;
        let src = TempDir::new().unwrap();
        let monitor = TestMonitor::arc();

        src.child("a").touch().unwrap();

        backup(
            &archive,
            src.path(),
            &backup::BackupOptions::default(),
            monitor.clone(),
        )
        .await
        .unwrap();

        // Use a sync Mutex here because this is a sync callback.
        let changes = Arc::new(Mutex::new(Vec::new()));
        let changes_clone = Arc::clone(&changes); // Clone to move into the closure below, which needs to be 'static
        let options = BackupOptions {
            change_callback: Some(Box::new(move |change| {
                changes_clone.lock().unwrap().push(change.clone());
                Ok(())
            })),
            ..BackupOptions::default()
        };

        std::fs::remove_file(src.child("a").path()).unwrap();
        let stats2 = backup(&archive, src.path(), &options, monitor.clone())
            .await
            .unwrap();

        assert_eq!(stats2.files, 0);
        assert_eq!(monitor.get_counter(Counter::EntriesDeleted), 1);
        assert_eq!(
            changes.lock().unwrap().len(),
            1,
            "should have seen a change for the deletion"
        );
        let change = &changes.lock().unwrap()[0];
        assert_eq!(change.to_string(), "- /a");
    }

    const HELLO_HASH: &str = "9063990e5c5b2184877f92adace7c801a549b00c39cd7549877f06d5dd0d3a6ca6eee42d5\
     896bdac64831c8114c55cee664078bd105dc691270c92644ccb2ce7";

    #[tokio::test]
    async fn simple_backup() -> Result<()> {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        srcdir.create_file("hello");

        let monitor = TestMonitor::arc();
        let backup_stats = backup(
            &af,
            srcdir.path(),
            &BackupOptions::default(),
            monitor.clone(),
        )
        .await
        .expect("backup");
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 1);
        assert_eq!(backup_stats.files, 1);
        assert_eq!(backup_stats.deduplicated_blocks, 0);
        assert_eq!(backup_stats.written_blocks, 1);
        assert_eq!(backup_stats.uncompressed_bytes, 8);
        assert_eq!(backup_stats.compressed_bytes, 10);
        check_backup(&af).await?;

        let restore_dir = TempDir::new().unwrap();

        let archive = Archive::open(af.transport().clone()).await.unwrap();
        assert!(archive.band_exists(BandId::zero()).await.unwrap());
        assert!(archive.band_is_closed(BandId::zero()).await.unwrap());
        assert!(!archive.band_exists(BandId::new(&[1])).await.unwrap());
        restore(
            &archive,
            restore_dir.path(),
            RestoreOptions::default(),
            monitor.clone(),
        )
        .await
        .expect("restore");

        monitor.assert_counter(Counter::FileBytes, 8);
        Ok(())
    }

    #[tokio::test]
    async fn simple_backup_with_excludes() -> Result<()> {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        srcdir.create_file("hello");
        srcdir.create_file("foooo");
        srcdir.create_file("bar");
        srcdir.create_file("baz");
        // TODO: Include a symlink only on Unix.
        let exclude = Exclude::from_strings(["/**/baz", "/**/bar", "/**/fooo*"]).unwrap();
        let options = BackupOptions {
            exclude,
            ..BackupOptions::default()
        };
        let monitor = TestMonitor::arc();
        let stats = backup(&af, srcdir.path(), &options, monitor.clone())
            .await
            .expect("backup");

        check_backup(&af).await?;

        let counters = monitor.counters();
        dbg!(counters);
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 1);
        assert_eq!(stats.files, 1);
        // TODO: Check stats for the number of excluded entries.
        assert!(counters.get(Counter::IndexWriteCompressedBytes) > 100);
        assert!(counters.get(Counter::IndexWriteUncompressedBytes) > 200);

        let restore_dir = TempDir::new().unwrap();

        let archive = Archive::open(af.transport().clone()).await.unwrap();

        let band = Band::open(&archive, BandId::zero()).await.unwrap();
        let band_info = band.get_info().await?;
        assert_eq!(band_info.index_hunk_count, Some(1));
        assert_eq!(band_info.id, BandId::zero());
        assert!(band_info.is_closed);
        assert!(band_info.end_time.is_some());

        let monitor = TestMonitor::arc();
        restore(
            &archive,
            restore_dir.path(),
            RestoreOptions::default(),
            monitor.clone(),
        )
        .await
        .expect("restore");
        monitor.assert_counter(Counter::FileBytes, 8);
        // TODO: Read back contents of that file.
        // TODO: Check index stats.
        // TODO: Check what was restored.

        af.validate(&ValidateOptions::default(), Arc::new(TestMonitor::new()))
            .await
            .unwrap();
        // TODO: Maybe check there were no errors or warnings.
        Ok(())
    }

    #[tokio::test]
    async fn backup_more_excludes() {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();

        srcdir.create_dir("test");
        srcdir.create_dir("foooooo");
        srcdir.create_file("foo");
        srcdir.create_file("fooBar");
        srcdir.create_file("foooooo/test");
        srcdir.create_file("test/baz");
        srcdir.create_file("baz");
        srcdir.create_file("bar");

        let exclude = Exclude::from_strings(["/**/foo*", "/**/baz"]).unwrap();
        let options = BackupOptions {
            exclude,
            ..Default::default()
        };
        let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
            .await
            .expect("backup");

        assert_eq!(1, stats.written_blocks);
        assert_eq!(1, stats.files);
        assert_eq!(1, stats.new_files);
        assert_eq!(2, stats.directories);
        assert_eq!(0, stats.symlinks);
        assert_eq!(0, stats.unknown_kind);
    }

    async fn check_backup(archive: &Archive) -> Result<()> {
        let band_ids = archive.list_band_ids().await.unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].to_string());
        assert_eq!(
            archive.last_complete_band().await.unwrap().unwrap().id(),
            BandId::new(&[0])
        );

        let band = Band::open(archive, band_ids[0]).await.unwrap();
        assert!(band.is_closed().await.unwrap());

        let index_entries = band
            .index()
            .iter_available_hunks()
            .await
            .collect_entry_vec()
            .await?;
        assert_eq!(2, index_entries.len());

        let root_entry = &index_entries[0];
        assert_eq!("/", root_entry.apath.to_string());
        assert_eq!(Kind::Dir, root_entry.kind);
        assert!(root_entry.mtime > 0);

        let file_entry = &index_entries[1];
        assert_eq!("/hello", file_entry.apath.to_string());
        assert_eq!(Kind::File, file_entry.kind);
        assert!(file_entry.mtime > 0);

        assert_eq!(
            archive
                .referenced_blocks(&archive.list_band_ids().await.unwrap(), TestMonitor::arc())
                .await
                .unwrap()
                .into_iter()
                .map(|h| h.to_string())
                .collect::<Vec<String>>(),
            vec![HELLO_HASH]
        );
        assert_eq!(
            archive
                .all_blocks()
                .await
                .unwrap()
                .iter()
                .map(|h| h.to_string())
                .collect::<Vec<String>>(),
            vec![HELLO_HASH]
        );
        assert_eq!(
            archive
                .unreferenced_blocks(TestMonitor::arc())
                .await
                .unwrap()
                .len(),
            0
        );
        Ok(())
    }

    #[tokio::test]
    async fn large_file() {
        let af = Archive::create_temp().await;
        let tf = TreeFixture::new();

        let file_size = 4 << 20;
        let large_content = vec![b'a'; file_size];
        tf.create_file_with_contents("large", &large_content);

        let monitor = TestMonitor::arc();
        let backup_stats = backup(
            &af,
            tf.path(),
            &BackupOptions {
                max_block_size: 1 << 20,
                ..Default::default()
            },
            monitor.clone(),
        )
        .await
        .expect("backup");
        assert_eq!(backup_stats.new_files, 1);
        // First 1MB should be new; remainder should be deduplicated.
        assert_eq!(backup_stats.uncompressed_bytes, 1 << 20);
        assert_eq!(backup_stats.written_blocks, 1);
        assert_eq!(backup_stats.deduplicated_blocks, 3);
        assert_eq!(backup_stats.deduplicated_bytes, 3 << 20);
        assert_eq!(backup_stats.errors, 0);
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 1);

        // Try to restore it
        let rd = TempDir::new().unwrap();
        let restore_archive = Archive::open(af.transport().clone()).await.unwrap();
        let monitor = TestMonitor::arc();
        restore(
            &restore_archive,
            rd.path(),
            RestoreOptions::default(),
            monitor.clone(),
        )
        .await
        .expect("restore");
        monitor.assert_no_errors();
        monitor.assert_counter(Counter::Files, 1);
        monitor.assert_counter(Counter::FileBytes, file_size);

        let content = std::fs::read(rd.path().join("large")).unwrap();
        assert_eq!(large_content, content);
    }

    /// If some files are unreadable, others are stored and the backup completes with warnings.
    #[tokio::test]
    #[cfg(unix)]
    async fn source_unreadable() {
        let af = Archive::create_temp().await;
        let tf = TreeFixture::new();

        tf.create_file("a");
        tf.create_file("b_unreadable");
        tf.create_file("c");

        tf.make_file_unreadable("b_unreadable");

        let stats = backup(
            &af,
            tf.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .expect("backup");
        assert_eq!(stats.errors, 1);
        assert_eq!(stats.new_files, 3);
        assert_eq!(stats.files, 3);

        // TODO: On Windows change the ACL to make the file unreadable to the current user or to
        // everyone.
    }

    /// Files from before the Unix epoch can be backed up.
    ///
    /// Reproduction of <https://github.com/sourcefrog/conserve/issues/100>.
    #[tokio::test]
    async fn mtime_before_epoch() {
        let tf = TreeFixture::new();
        let file_path = tf.create_file("old_file");

        let t1969 = FileTime::from_unix_time(-36_000, 0);
        set_file_mtime(file_path, t1969).expect("Failed to set file times");

        let lt = SourceTree::open(tf.path()).unwrap();
        let monitor = TestMonitor::arc();
        let entries = lt
            .iter_entries(Apath::root(), Exclude::nothing(), monitor.clone())
            .unwrap()
            .collect::<Vec<_>>();

        assert_eq!(entries[0].apath(), "/");
        assert_eq!(entries[1].apath(), "/old_file");

        let af = Archive::create_temp().await;
        backup(
            &af,
            tf.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .expect("backup shouldn't crash on before-epoch mtimes");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn symlink() -> Result<()> {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        srcdir.create_symlink("symlink", "/a/broken/destination");

        let copy_stats = backup(
            &af,
            srcdir.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .expect("backup");

        assert_eq!(0, copy_stats.files);
        assert_eq!(1, copy_stats.symlinks);
        assert_eq!(0, copy_stats.unknown_kind);

        let band_ids = af.list_band_ids().await.unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].to_string());

        let band = Band::open(&af, band_ids[0]).await.unwrap();
        assert!(band.is_closed().await.unwrap());

        let index_entries = band
            .index()
            .iter_available_hunks()
            .await
            .collect_entry_vec()
            .await?;
        assert_eq!(2, index_entries.len());

        let e2 = &index_entries[1];
        assert_eq!(e2.kind(), Kind::Symlink);
        assert_eq!(&e2.apath, "/symlink");
        assert_eq!(e2.target.as_ref().unwrap(), "/a/broken/destination");
        Ok(())
    }

    #[tokio::test]
    async fn empty_file_uses_zero_blocks() {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        srcdir.create_file_with_contents("empty", &[]);
        let stats = backup(
            &af,
            srcdir.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .unwrap();

        assert_eq!(1, stats.files);
        assert_eq!(stats.written_blocks, 0);

        // Read back the empty file
        let st = af
            .open_stored_tree(BandSelectionPolicy::Latest)
            .await
            .unwrap();
        let entries = st
            .iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
            .collect_all()
            .await
            .unwrap();
        let empty_entry = entries
            .iter()
            .find(|i| &i.apath == "/empty")
            .expect("found one entry");
        assert_eq!(empty_entry.addrs, []);

        // Restore it
        let dest = TempDir::new().unwrap();
        restore(
            &af,
            dest.path(),
            RestoreOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .expect("restore");
        // TODO: Check restore stats.
        dest.child("empty").assert("");
    }

    #[tokio::test]
    async fn detect_unmodified() {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        srcdir.create_file("aaa");
        srcdir.create_file("bbb");

        let options = BackupOptions::default();
        let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
            .await
            .unwrap();

        assert_eq!(stats.files, 2);
        assert_eq!(stats.new_files, 2);
        assert_eq!(stats.unmodified_files, 0);

        // Make a second backup from the same tree, and we should see that
        // both files are unmodified.
        let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
            .await
            .unwrap();

        assert_eq!(stats.files, 2);
        assert_eq!(stats.new_files, 0);
        assert_eq!(stats.unmodified_files, 2);

        // Change one of the files, and in a new backup it should be recognized
        // as unmodified.
        srcdir.create_file_with_contents("bbb", b"longer content for bbb");

        let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
            .await
            .unwrap();

        assert_eq!(stats.files, 2);
        assert_eq!(stats.new_files, 0);
        assert_eq!(stats.unmodified_files, 1);
        assert_eq!(stats.modified_files, 1);
    }

    #[tokio::test]
    async fn detect_minimal_mtime_change() {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        srcdir.create_file("aaa");
        srcdir.create_file_with_contents("bbb", b"longer content for bbb");

        let options = BackupOptions::default();
        let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
            .await
            .unwrap();

        assert_eq!(stats.files, 2);
        assert_eq!(stats.new_files, 2);
        assert_eq!(stats.unmodified_files, 0);
        assert_eq!(stats.modified_files, 0);

        // Spin until the file's mtime is visibly different to what it was before.
        let bpath = srcdir.path().join("bbb");
        let orig_mtime = std::fs::metadata(&bpath).unwrap().modified().unwrap();
        loop {
            // Sleep a little while, so that even on systems with less than
            // nanosecond filesystem time resolution we can still see this is later.
            std::thread::sleep(std::time::Duration::from_millis(50));
            // Change one of the files, keeping the same length. If the mtime
            // changed, even fractionally, we should see the file was changed.
            srcdir.create_file_with_contents("bbb", b"woofer content for bbb");
            if std::fs::metadata(&bpath).unwrap().modified().unwrap() != orig_mtime {
                break;
            }
        }

        let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
            .await
            .unwrap();
        assert_eq!(stats.files, 2);
        assert_eq!(stats.unmodified_files, 1);
    }

    #[tokio::test]
    async fn small_files_combined_two_backups() {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        srcdir.create_file("file1");
        srcdir.create_file("file2");

        let stats1 = backup(
            &af,
            srcdir.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .unwrap();
        // Although the two files have the same content, we do not yet dedupe them
        // within a combined block, so the block is different to when one identical
        // file is stored alone. This could be fixed.
        assert_eq!(stats1.combined_blocks, 1);
        assert_eq!(stats1.new_files, 2);
        assert_eq!(stats1.written_blocks, 1);
        assert_eq!(stats1.new_files, 2);

        // Add one more file, also identical, but it is not combined with the previous blocks.
        // This is a shortcoming of the current dedupe approach.
        srcdir.create_file("file3");
        let stats2 = backup(
            &af,
            srcdir.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .unwrap();
        assert_eq!(stats2.new_files, 1);
        assert_eq!(stats2.unmodified_files, 2);
        assert_eq!(stats2.written_blocks, 1);
        assert_eq!(stats2.combined_blocks, 1);

        assert_eq!(af.all_blocks().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn many_small_files_combined_to_one_block() {
        // tracing_subscriber::fmt::init();
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        // The directory also counts as an entry, so we should be able to fit 1999
        // files in 2 hunks of 1000 entries.
        for i in 0..1999 {
            srcdir.create_file_of_length_with_prefix(
                &format!("file{i:04}"),
                200,
                format!("something about {i}").as_bytes(),
            );
        }
        let backup_options = BackupOptions {
            max_entries_per_hunk: 1000,
            ..Default::default()
        };
        let monitor = TestMonitor::arc();
        let stats = backup(&af, srcdir.path(), &backup_options, monitor.clone())
            .await
            .expect("backup");
        assert_eq!(
            monitor.get_counter(Counter::IndexWrites),
            2,
            "expect exactly 2 hunks"
        );
        assert_eq!(stats.files, 1999);
        assert_eq!(stats.directories, 1);
        assert_eq!(stats.unknown_kind, 0);

        assert_eq!(stats.new_files, 1999);
        assert_eq!(stats.small_combined_files, 1999);
        assert_eq!(stats.errors, 0);
        // We write two combined blocks
        assert_eq!(stats.written_blocks, 2);
        assert_eq!(stats.combined_blocks, 2);

        let tree = af
            .open_stored_tree(BandSelectionPolicy::Latest)
            .await
            .unwrap();
        let entries = tree
            .iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
            .collect_all()
            .await
            .unwrap();
        assert_eq!(entries[0].apath(), "/");
        for (i, entry) in entries.iter().skip(1).enumerate() {
            assert_eq!(entry.apath().to_string(), format!("/file{i:04}"));
        }
        assert_eq!(entries.len(), 2000);
    }

    #[tokio::test]
    async fn mixed_medium_small_files_two_hunks() {
        // tracing_subscriber::fmt::init();

        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        const MEDIUM_LENGTH: u64 = 150_000;
        // Make some files large enough not to be grouped together as small files.
        for i in 0..1999 {
            let name = format!("file{i:04}");
            if i % 100 == 0 {
                srcdir.create_file_of_length_with_prefix(&name, MEDIUM_LENGTH, b"something");
            } else {
                srcdir.create_file(&name);
            }
        }
        let backup_options = BackupOptions {
            max_entries_per_hunk: 1000,
            small_file_cap: 100_000,
            ..Default::default()
        };
        let monitor = TestMonitor::arc();
        let stats = backup(&af, srcdir.path(), &backup_options, monitor.clone())
            .await
            .expect("backup");
        assert_eq!(
            monitor.get_counter(Counter::IndexWrites),
            2,
            "expect exactly 2 hunks"
        );
        assert_eq!(stats.files, 1999);
        assert_eq!(stats.directories, 1);
        assert_eq!(stats.unknown_kind, 0);

        assert_eq!(stats.new_files, 1999);
        assert_eq!(stats.single_block_files, 20);
        assert_eq!(stats.small_combined_files, 1999 - 20);
        assert_eq!(stats.errors, 0);
        // There's one deduped block for all the large files, and then one per hunk for all the small combined files.
        assert_eq!(stats.written_blocks, 3);

        let tree = af
            .open_stored_tree(BandSelectionPolicy::Latest)
            .await
            .unwrap();
        let entries = tree
            .iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
            .collect_all()
            .await
            .unwrap();
        assert_eq!(entries[0].apath(), "/");
        for (i, entry) in entries.iter().skip(1).enumerate() {
            assert_eq!(entry.apath().to_string(), format!("/file{i:04}"));
        }
        assert_eq!(entries.len(), 2000);
    }

    #[tokio::test]
    async fn detect_unchanged_from_stitched_index() {
        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();
        srcdir.create_file("a");
        srcdir.create_file("b");
        // Use small hunks for easier manipulation.
        let monitor = TestMonitor::arc();
        let stats = backup(
            &af,
            srcdir.path(),
            &BackupOptions {
                max_entries_per_hunk: 1,
                ..Default::default()
            },
            monitor.clone(),
        )
        .await
        .unwrap();
        assert_eq!(stats.new_files, 2);
        assert_eq!(stats.small_combined_files, 2);
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 3,);

        // Make a second backup, with the first file changed.
        let monitor = TestMonitor::arc();
        srcdir.create_file_with_contents("a", b"new a contents");
        let stats = backup(
            &af,
            srcdir.path(),
            &BackupOptions {
                max_entries_per_hunk: 1,
                ..Default::default()
            },
            monitor.clone(),
        )
        .await
        .unwrap();
        assert_eq!(stats.unmodified_files, 1);
        assert_eq!(stats.modified_files, 1);
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 3,);

        // Delete the last hunk and reopen the last band.
        af.transport().remove_file("b0001/BANDTAIL").await.unwrap();
        af.transport()
            .remove_file("b0001/i/00000/000000002")
            .await
            .unwrap();

        // The third backup should see nothing changed, by looking at the stitched
        // index from both b0 and b1.
        let monitor = TestMonitor::arc();
        let stats = backup(
            &af,
            srcdir.path(),
            &BackupOptions {
                max_entries_per_hunk: 1,
                ..Default::default()
            },
            monitor.clone(),
        )
        .await
        .unwrap();
        assert_eq!(stats.unmodified_files, 2, "both files are unmodified");
        assert_eq!(monitor.get_counter(Counter::IndexWrites), 3);
    }

    #[tokio::test]
    async fn unmodified_file_blocks_are_not_written_or_individually_checked() {
        let transport = Transport::temp().enable_record_calls();
        let archive = Archive::create(transport.clone()).await.unwrap();
        let srcdir = TreeFixture::new();
        srcdir.create_file("aaa");
        srcdir.create_file("bbb");

        let options = BackupOptions::default();
        let stats = backup(&archive, srcdir.path(), &options, TestMonitor::arc())
            .await
            .unwrap();

        assert_eq!(stats.files, 2);
        assert_eq!(stats.new_files, 2);
        assert_eq!(stats.unmodified_files, 0);
        let recording = transport.take_recording();
        println!("calls for first backup: {recording:#?}");

        // Reopen the archive to avoid cache effects.
        let archive = Archive::open(transport.clone()).await.unwrap();
        // Make a second backup from the same tree, and we should see that both files are unmodified.
        let stats = backup(&archive, srcdir.path(), &options, TestMonitor::arc())
            .await
            .unwrap();

        assert_eq!(stats.files, 2);
        assert_eq!(stats.new_files, 0);
        assert_eq!(stats.unmodified_files, 2);
        let recording = transport.take_recording();
        println!("calls for second backup without modification: {recording:#?}");
        let writes = recording.verb_paths(Verb::Write);
        println!("writes for second backup without modification: {writes:#?}");
        assert_eq!(
            writes,
            vec![
                "b0001/BANDHEAD",
                "b0001/i/00000/000000000",
                "b0001/BANDTAIL"
            ],
            "with no modification, backup should only write head, tail, and index"
        );
        let metadata_calls = recording.verb_paths(Verb::Metadata);
        println!("metadata calls for second backup without modification: {metadata_calls:#?}");
        assert_eq!(
            metadata_calls,
            ["GC_LOCK", "b0000/BANDTAIL"],
            "don't get metadata for data blocks"
        );
        assert!(
            metadata_calls
                .iter()
                .all(|c| c.ends_with("BANDTAIL") || c.ends_with("GC_LOCK"))
        );

        // Change one of the files, and in a new backup it should be recognized
        // as unmodified.
        let archive = Archive::open(transport.clone()).await.unwrap();
        srcdir.create_file_with_contents("bbb", b"longer content for bbb");
        let stats = backup(&archive, srcdir.path(), &options, TestMonitor::arc())
            .await
            .unwrap();

        assert_eq!(stats.files, 2);
        assert_eq!(stats.new_files, 0);
        assert_eq!(stats.unmodified_files, 1);
        assert_eq!(stats.modified_files, 1);

        let recording = transport.take_recording();
        println!("calls for third backup after modification: {recording:#?}");
        let writes = recording.verb_paths(Verb::Write);
        println!("writes for third backup after modification: {writes:#?}");
        assert_eq!(
            writes.len(),
            4,
            "write should be head, tail, index, and one data block: {writes:#?}"
        );
        let metadata_calls = recording.verb_paths(Verb::Metadata);
        println!("metadata calls for third backup after modification: {metadata_calls:#?}");
        assert_eq!(
            metadata_calls,
            ["GC_LOCK", "b0001/BANDTAIL"],
            "with modification to one file, backup is expected to get metadata for lock and previous band tail: {metadata_calls:#?}"
        );
    }
}
