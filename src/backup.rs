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
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
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
#[allow(clippy::while_let_on_iterator)] // will later be async
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
        let hunks = self.index_writer.finish()?;
        trace!(?hunks, "Closing band");
        self.band.close(hunks as u64)?;
        Ok(BackupStats { ..self.stats })
    }

    /// Write out any pending data blocks, and then the pending index entries.
    async fn flush_group(&mut self, monitor: Arc<dyn Monitor>) -> Result<()> {
        let (stats, mut entries) = self.file_combiner.drain(monitor.clone()).await?;
        trace!("Got {} entries to write from file combiner", entries.len());
        self.stats += stats;
        self.index_writer.append_entries(&mut entries);
        self.index_writer.finish_hunk()
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
                if all_blocks_present(&basis_entry.addrs, &self.block_dir, &monitor).await {
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
                    warn!(%apath, "Some referenced blocks are missing or truncated; file will be stored again");
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

async fn all_blocks_present(
    addresses: &[Address],
    block_dir: &BlockDir,
    monitor: &Arc<dyn Monitor>,
) -> bool {
    for addr in addresses {
        if !block_dir
            .contains(&addr.hash, monitor.clone())
            .await
            .unwrap_or(false)
        {
            return false;
        }
    }
    true
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
    use std::sync::Mutex;

    use assert_fs::prelude::*;
    use assert_fs::TempDir;

    use crate::monitor::test::TestMonitor;
    use crate::transport::Transport;

    use super::*;

    #[tokio::test]
    async fn deleted_files_are_reported() {
        // tracing_subscriber::fmt::init();

        let transport = Transport::temp();
        let archive = Archive::create(transport.clone()).unwrap();
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
}
