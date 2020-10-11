// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

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

use globset::GlobSet;

use crate::blockdir::StoreFiles;
use crate::index::IndexEntryIter;
use crate::stats::CopyStats;
use crate::tree::ReadTree;
use crate::*;

/// Configuration of how to make a backup.
#[derive(Debug, Default)]
pub struct BackupOptions {
    /// Print filenames to the UI as they're copied.
    pub print_filenames: bool,

    /// Exclude these globs from the backup.
    pub excludes: Option<GlobSet>,
}

/// Backup a source directory into a new band in the archive.
///
/// Returns statistics about what was copied.
pub fn backup(archive: &Archive, source: &LiveTree, options: &BackupOptions) -> Result<CopyStats> {
    let mut writer = BackupWriter::begin(archive)?;
    let mut stats = CopyStats::default();
    let mut progress_bar = ProgressBar::new();
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

    progress_bar.set_phase("Copying".to_owned());
    let entry_iter = source.iter_filtered(None, options.excludes.clone())?;
    for entry in entry_iter {
        if options.print_filenames {
            crate::ui::println(entry.apath());
        }
        progress_bar.set_filename(entry.apath().to_string());
        if let Err(e) = match entry.kind() {
            Kind::Dir => {
                stats.directories += 1;
                writer.copy_dir(&entry)
            }
            Kind::File => {
                stats.files += 1;
                let result = writer.copy_file(&entry, source);
                if let Ok(file_stats) = &result {
                    stats += file_stats.clone()
                }
                if let Some(bytes) = entry.size() {
                    progress_bar.increment_bytes_done(bytes);
                }
                result.and(Ok(()))
            }
            Kind::Symlink => {
                stats.symlinks += 1;
                writer.copy_symlink(&entry)
            }
            Kind::Unknown => {
                stats.unknown_kind += 1;
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                continue;
            }
        } {
            ui::show_error(&e);
            stats.errors += 1;
            continue;
        }
    }
    stats += writer.finish()?;
    // TODO: Merge in stats from the tree iter and maybe the source tree?
    Ok(stats)
}

/// Accepts files to write in the archive (in apath order.)
pub struct BackupWriter {
    band: Band,
    index_builder: IndexBuilder,
    store_files: StoreFiles,

    /// The index for the last stored band, used as hints for whether newly
    /// stored files have changed.
    basis_index: Option<IndexEntryIter>,
}

impl BackupWriter {
    /// Create a new BackupWriter.
    ///
    /// This currently makes a new top-level band.
    pub fn begin(archive: &Archive) -> Result<BackupWriter> {
        if gc_lock::GarbageCollectionLock::is_locked(archive)? {
            return Err(Error::GarbageCollectionLockHeld);
        }
        let basis_index = archive
            .last_complete_band()?
            .map(|b| b.iter_entries())
            .transpose()?;
        // Create the new band only after finding the basis band!
        let band = Band::create(archive)?;
        let index_builder = band.index_builder();
        Ok(BackupWriter {
            band,
            index_builder,
            store_files: StoreFiles::new(archive.block_dir().clone()),
            basis_index,
        })
    }

    fn finish(self) -> Result<CopyStats> {
        let index_builder_stats = self.index_builder.finish()?;
        self.band.close(index_builder_stats.index_hunks)?;
        Ok(CopyStats {
            index_builder_stats,
            ..CopyStats::default()
        })
    }

    fn copy_dir<E: Entry>(&mut self, source_entry: &E) -> Result<()> {
        // TODO: Pass back index sizes
        self.index_builder
            .push_entry(IndexEntry::metadata_from(source_entry))
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file<R: ReadTree>(
        &mut self,
        source_entry: &R::Entry,
        from_tree: &R,
    ) -> Result<CopyStats> {
        let mut stats = CopyStats::default();
        let apath = source_entry.apath();
        if let Some(basis_entry) = self
            .basis_index
            .as_mut()
            .map(|bi| bi.advance_to(&apath))
            .flatten()
        {
            if source_entry.is_unchanged_from(&basis_entry) {
                // TODO: In verbose mode, say if the file is changed, unchanged,
                // etc, but without duplicating the filenames.
                //
                // ui::println(&format!("unchanged file {}", apath));

                // We can reasonably assume that the existing archive complies
                // with the archive invariants, which include that all the
                // blocks referenced by the index, are actually present.
                stats.unmodified_files += 1;
                self.index_builder.push_entry(basis_entry)?;
                return Ok(stats);
            } else {
                stats.modified_files += 1;
            }
        } else {
            stats.new_files += 1;
        }
        let content = &mut from_tree.file_contents(&source_entry)?;
        // TODO: Don't read the whole file into memory, but especially don't do that and
        // then downcast it to Read.
        let (addrs, file_stats) = self.store_files.store_file_content(&apath, content)?;
        stats += file_stats;
        self.index_builder.push_entry(IndexEntry {
            addrs,
            ..IndexEntry::metadata_from(source_entry)
        })?;
        Ok(stats)
    }

    fn copy_symlink<E: Entry>(&mut self, source_entry: &E) -> Result<()> {
        let target = source_entry.symlink_target().clone();
        assert!(target.is_some());
        self.index_builder
            .push_entry(IndexEntry::metadata_from(source_entry))
    }
}
