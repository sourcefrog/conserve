// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2021, 2022 Martin Pool.

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

use std::io::prelude::*;
use std::{convert::TryInto, time::Instant};

use itertools::Itertools;
use tracing::{debug, Level};

use crate::blockdir::Address;
use crate::io::read_with_retries;
use crate::monitor::DefaultMonitor;
use crate::stats::BackupStats;
use crate::stitch::IterStitchedIndexHunks;
use crate::tree::ReadTree;
use crate::*;

/// Configuration of how to make a backup.
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Print filenames to the UI as they're copied.
    pub print_filenames: bool,

    /// Exclude these globs from the backup.
    pub exclude: Exclude,

    pub max_entries_per_hunk: usize,
}

impl Default for BackupOptions {
    fn default() -> BackupOptions {
        BackupOptions {
            print_filenames: false,
            exclude: Exclude::nothing(),
            max_entries_per_hunk: crate::index::MAX_ENTRIES_PER_HUNK,
        }
    }
}

/// Backup a source directory into a new band in the archive.
///
/// Returns statistics about what was copied.
pub fn backup(
    archive: &Archive,
    source: &LiveTree,
    options: &BackupOptions,
    monitor: Option<&dyn BackupMonitor>,
) -> Result<BackupStats> {
    let _span = tracing::span!(Level::DEBUG, "backup");

    let mut default_monitor = DefaultMonitor{};
    let monitor = monitor.unwrap_or(&mut default_monitor);

    let start = Instant::now();
    let mut writer = BackupWriter::begin(archive)?;
    let mut stats = BackupStats::default();

    let entry_iter = source.iter_entries(Apath::root(), options.exclude.clone())?;
    for entry_group in entry_iter.chunks(options.max_entries_per_hunk).into_iter() {
        for entry in entry_group {
            monitor.copy(&entry);
            
            match writer.copy_entry(&entry, source) {
                Err(e) => {
                    for line in ui::format_error_causes(&e).lines() {
                        debug!("{}", line);
                    }
                    
                    monitor.copy_error(&entry, &e);
                    stats.errors += 1;
                    continue;
                }
                Ok(result) => {
                    monitor.copy_result(&entry, &result);
                }
            }
        }
        writer.flush_group()?;
    }

    stats += writer.finish()?;
    stats.elapsed = start.elapsed();

    // TODO: Merge in stats from the source tree?

    monitor.finished(&stats);
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
    basis_index: Option<crate::index::IndexEntryIter<crate::stitch::IterStitchedIndexHunks>>,

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
        let basis_index = archive.last_band_id()?.map(|band_id| {
            IterStitchedIndexHunks::new(archive, &band_id)
                .iter_entries(Apath::root(), Exclude::nothing())
        });
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
    fn copy_entry(&mut self, entry: &LiveEntry, source: &LiveTree) -> Result<Option<DiffKind>> {
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

    fn copy_dir<E: Entry>(&mut self, source_entry: &E) -> Result<Option<DiffKind>> {
        self.stats.directories += 1;
        self.index_builder
            .push_entry(IndexEntry::metadata_from(source_entry));
        Ok(None) // TODO: See if it changed from the basis?
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file(
        &mut self,
        source_entry: &LiveEntry,
        from_tree: &LiveTree,
    ) -> Result<Option<DiffKind>> {
        self.stats.files += 1;
        let apath = source_entry.apath();
        let result;
        if let Some(basis_entry) = self
            .basis_index
            .as_mut()
            .and_then(|bi| bi.advance_to(apath))
        {
            if source_entry.is_unchanged_from(&basis_entry) {
                self.stats.unmodified_files += 1;
                self.index_builder.push_entry(basis_entry);
                return Ok(Some(DiffKind::Unchanged));
            } else {
                self.stats.modified_files += 1;
                result = Some(DiffKind::Changed);
            }
        } else {
            self.stats.new_files += 1;
            result = Some(DiffKind::New);
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

    fn copy_symlink<E: Entry>(&mut self, source_entry: &E) -> Result<Option<DiffKind>> {
        let target = source_entry.symlink_target().clone();
        self.stats.symlinks += 1;
        assert!(target.is_some());
        self.index_builder
            .push_entry(IndexEntry::metadata_from(source_entry));
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
