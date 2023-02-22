// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

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

use std::convert::TryInto;
use std::fmt;
use std::io::prelude::*;
use std::time::{Duration, Instant};

use derive_more::{Add, AddAssign};
use itertools::Itertools;
use tracing::error;

use crate::blockdir::Address;
use crate::change::Change;
use crate::io::read_with_retries;
use crate::progress::{Bar, Progress};
use crate::stats::{
    write_compressed_size, write_count, write_duration, write_size, IndexWriterStats,
};
use crate::stitch::IterStitchedIndexHunks;
use crate::tree::ReadTree;
use crate::*;

/// Configuration of how to make a backup.
pub struct BackupOptions<'cb> {
    /// Exclude these globs from the backup.
    pub exclude: Exclude,

    pub max_entries_per_hunk: usize,

    // Call this callback as each entry is successfully stored.
    pub change_callback: Option<ChangeCallback<'cb>>,
}

impl Default for BackupOptions<'_> {
    fn default() -> BackupOptions<'static> {
        BackupOptions {
            exclude: Exclude::nothing(),
            max_entries_per_hunk: crate::index::MAX_ENTRIES_PER_HUNK,
            change_callback: None,
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
pub fn backup(
    archive: &Archive,
    source: &LiveTree,
    options: &BackupOptions,
) -> Result<BackupStats> {
    let start = Instant::now();
    let mut writer = BackupWriter::begin(archive)?;
    let mut stats = BackupStats::default();
    let bar = Bar::new();

    let mut scanned_dirs = 0;
    let mut scanned_files = 0;
    let mut scanned_file_bytes = 0;
    let mut entries_new = 0;
    let mut entries_changed = 0;
    let mut entries_unchanged = 0;

    let entry_iter = source.iter_entries(Apath::root(), options.exclude.clone())?;
    for entry_group in entry_iter.chunks(options.max_entries_per_hunk).into_iter() {
        for entry in entry_group {
            match entry.kind() {
                Kind::Dir => scanned_dirs += 1,
                Kind::File => scanned_files += 1,
                _ => (),
            }
            match writer.copy_entry(&entry, source) {
                Err(err) => {
                    error!(?entry, ?err, "Error copying entry to backup");
                    stats.errors += 1;
                    continue;
                }
                Ok(Some(entry_change)) => {
                    match entry_change.change {
                        Change::Changed { .. } => entries_changed += 1,
                        Change::Added { .. } => entries_new += 1,
                        Change::Unchanged { .. } => entries_unchanged += 1,
                        // Deletions are not produced at the moment.
                        Change::Deleted { .. } => (), // model.entries_deleted += 1,
                    }
                    if let Some(cb) = &options.change_callback {
                        cb(&entry_change)?;
                    }
                }
                Ok(_) => {}
            }
            if let Some(bytes) = entry.size() {
                if bytes > 0 {
                    scanned_file_bytes += bytes;
                    bar.post(Progress::Backup {
                        filename: entry.apath().to_string(),
                        scanned_file_bytes,
                        scanned_dirs,
                        scanned_files,
                        entries_new,
                        entries_changed,
                        entries_unchanged,
                    });
                }
            }
        }
        writer.flush_group()?;
    }
    stats += writer.finish()?;
    stats.elapsed = start.elapsed();
    // TODO: Merge in stats from the source tree?
    Ok(stats)
}

/// Accepts files to write in the archive (in apath order.)
struct BackupWriter {
    band: Band,
    index_builder: IndexWriter,
    stats: BackupStats,
    block_dir: BlockDir,

    /// The index for the last stored band, used as hints for whether newly
    /// stored files have changed.
    basis_index: crate::index::IndexEntryIter<crate::stitch::IterStitchedIndexHunks>,

    file_combiner: FileCombiner,
}

impl BackupWriter {
    /// Create a new BackupWriter.
    ///
    /// This currently makes a new top-level band.
    pub fn begin(archive: &Archive) -> Result<BackupWriter> {
        if gc_lock::GarbageCollectionLock::is_locked(archive)? {
            return Err(Error::GarbageCollectionLockHeld);
        }
        let basis_index = IterStitchedIndexHunks::new(archive, archive.last_band_id()?)
            .iter_entries(Apath::root(), Exclude::nothing());

        // Create the new band only after finding the basis band!
        let band = Band::create(archive)?;
        let index_builder = band.index_builder();
        Ok(BackupWriter {
            band,
            index_builder,
            block_dir: archive.block_dir().clone(),
            stats: BackupStats::default(),
            basis_index,
            file_combiner: FileCombiner::new(archive.block_dir().clone()),
        })
    }

    fn finish(self) -> Result<BackupStats> {
        let index_builder_stats = self.index_builder.finish()?;
        self.band.close(index_builder_stats.index_hunks as u64)?;
        Ok(BackupStats {
            index_builder_stats,
            ..self.stats
        })
    }

    /// Write out any pending data blocks, and then the pending index entries.
    fn flush_group(&mut self) -> Result<()> {
        let (stats, mut entries) = self.file_combiner.drain()?;
        self.stats += stats;
        self.index_builder.append_entries(&mut entries);
        self.index_builder.finish_hunk()
    }

    /// Add one entry to the backup.
    ///
    /// Return an indication of whether it changed (if it's a file), or
    /// None for non-plain-file types where that information is not currently
    /// calculated.
    fn copy_entry(&mut self, entry: &LiveEntry, source: &LiveTree) -> Result<Option<EntryChange>> {
        // TODO: Emit deletions for entries in the basis not present in the source.
        match entry.kind() {
            Kind::Dir => self.copy_dir(entry),
            Kind::File => self.copy_file(entry, source),
            Kind::Symlink => self.copy_symlink(entry),
            Kind::Unknown => {
                self.stats.unknown_kind += 1;
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                Ok(None)
            }
        }
    }

    fn copy_dir<E: Entry>(&mut self, source_entry: &E) -> Result<Option<EntryChange>> {
        self.stats.directories += 1;
        self.index_builder
            .push_entry(IndexEntry::metadata_from(source_entry));
        Ok(None) // TODO: Emit the actual change.
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file(
        &mut self,
        source_entry: &LiveEntry,
        from_tree: &LiveTree,
    ) -> Result<Option<EntryChange>> {
        self.stats.files += 1;
        let apath = source_entry.apath();
        let result;
        if let Some(basis_entry) = self.basis_index.advance_to(apath) {
            if entry_metadata_unchanged(source_entry, &basis_entry) {
                self.stats.unmodified_files += 1;
                let change = Some(EntryChange::unchanged(&basis_entry));
                self.index_builder.push_entry(basis_entry);
                return Ok(change);
            } else {
                self.stats.modified_files += 1;
                result = Some(EntryChange::changed(&basis_entry, source_entry));
            }
        } else {
            self.stats.new_files += 1;
            result = Some(EntryChange::added(source_entry));
        }
        let mut read_source = from_tree.file_contents(source_entry)?;
        let size = source_entry.size().expect("LiveEntry has a size");
        if size == 0 {
            self.index_builder
                .push_entry(IndexEntry::metadata_from(source_entry));
            self.stats.empty_files += 1;
            return Ok(result);
        }
        if size <= SMALL_FILE_CAP {
            self.file_combiner
                .push_file(source_entry, &mut read_source)?;
            return Ok(result);
        }
        let addrs = store_file_content(
            apath,
            &mut read_source,
            &mut self.block_dir,
            &mut self.stats,
        )?;
        self.index_builder.push_entry(IndexEntry {
            addrs,
            ..IndexEntry::metadata_from(source_entry)
        });
        Ok(result)
    }

    fn copy_symlink<E: Entry>(&mut self, source_entry: &E) -> Result<Option<EntryChange>> {
        let target = source_entry.symlink_target().clone();
        self.stats.symlinks += 1;
        assert!(target.is_some());
        self.index_builder
            .push_entry(IndexEntry::metadata_from(source_entry));
        // TODO: Emit the actual change.
        Ok(None)
    }
}

fn store_file_content(
    apath: &Apath,
    from_file: &mut dyn Read,
    block_dir: &mut BlockDir,
    stats: &mut BackupStats,
) -> Result<Vec<Address>> {
    let mut buffer = Vec::new();
    let mut addresses = Vec::<Address>::with_capacity(1);
    loop {
        read_with_retries(&mut buffer, MAX_BLOCK_SIZE, from_file).map_err(|source| {
            Error::StoreFile {
                apath: apath.to_owned(),
                source,
            }
        })?;
        if buffer.is_empty() {
            break;
        }
        let hash = block_dir.store_or_deduplicate(buffer.as_slice(), stats)?;
        addresses.push(Address {
            hash,
            start: 0,
            len: buffer.len() as u64,
        });
    }
    match addresses.len() {
        0 => stats.empty_files += 1,
        1 => stats.single_block_files += 1,
        _ => stats.multi_block_files += 1,
    }
    Ok(addresses)
}

/// Combines multiple small files into a single block.
///
/// When the block is finished, and only then, this returns the index entries with the addresses
/// completed.
struct FileCombiner {
    /// Buffer of concatenated data from small files.
    buf: Vec<u8>,
    queue: Vec<QueuedFile>,
    /// Entries for files that have been written to the blockdir, and that have complete addresses.
    finished: Vec<IndexEntry>,
    stats: BackupStats,
    block_dir: BlockDir,
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
    fn new(block_dir: BlockDir) -> FileCombiner {
        FileCombiner {
            block_dir,
            buf: Vec::new(),
            queue: Vec::new(),
            finished: Vec::new(),
            stats: BackupStats::default(),
        }
    }

    /// Flush any pending files, and return accumulated file entries and stats.
    /// The FileCombiner is then empty and ready for reuse.
    fn drain(&mut self) -> Result<(BackupStats, Vec<IndexEntry>)> {
        self.flush()?;
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
    fn flush(&mut self) -> Result<()> {
        if self.queue.is_empty() {
            debug_assert!(self.buf.is_empty());
            return Ok(());
        }
        let hash = self
            .block_dir
            .store_or_deduplicate(&self.buf, &mut self.stats)?;
        self.stats.combined_blocks += 1;
        self.buf.clear();
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
    fn push_file(&mut self, live_entry: &LiveEntry, from_file: &mut dyn Read) -> Result<()> {
        let start = self.buf.len();
        let expected_len: usize = live_entry
            .size()
            .expect("small file has no length")
            .try_into()
            .unwrap();
        let index_entry = IndexEntry::metadata_from(live_entry);
        if expected_len == 0 {
            self.stats.empty_files += 1;
            self.finished.push(index_entry);
            return Ok(());
        }
        self.buf.resize(start + expected_len, 0);
        let len = from_file
            .read(&mut self.buf[start..])
            .map_err(|source| Error::StoreFile {
                apath: live_entry.apath().to_owned(),
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
        if self.buf.len() >= TARGET_COMBINED_BLOCK_SIZE {
            self.flush()
        } else {
            Ok(())
        }
    }
}

/// True if the metadata supports an assumption the file contents have
/// not changed.
fn entry_metadata_unchanged<E: Entry, O: Entry>(new_entry: &E, basis_entry: &O) -> bool {
    basis_entry.kind() == new_entry.kind()
        && basis_entry.mtime() == new_entry.mtime()
        && basis_entry.size() == new_entry.size()
        && basis_entry.unix_mode() == new_entry.unix_mode()
        && basis_entry.owner() == new_entry.owner()
}

#[derive(Add, AddAssign, Debug, Default, Eq, PartialEq, Clone)]
pub struct BackupStats {
    // TODO: Have separate more-specific stats for backup and restore, and then
    // each can have a single Display method.
    // TODO: Include source file bytes, including unmodified files.
    pub files: usize,
    pub symlinks: usize,
    pub directories: usize,
    pub unknown_kind: usize,

    pub unmodified_files: usize,
    pub modified_files: usize,
    pub new_files: usize,

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

    pub index_builder_stats: IndexWriterStats,
    pub elapsed: Duration,
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

        let idx = &self.index_builder_stats;
        write_count(w, "new index hunks", idx.index_hunks);
        write_compressed_size(w, idx.compressed_index_bytes, idx.uncompressed_index_bytes);
        writeln!(w).unwrap();

        write_count(w, "errors", self.errors);
        write_duration(w, "elapsed", self.elapsed)?;

        Ok(())
    }
}
