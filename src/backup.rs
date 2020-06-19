// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use globset::GlobSet;

use crate::blockdir::StoreFiles;
use crate::index::IndexEntryIter;
use crate::stats::CopyStats;
use crate::*;

/// Configuration of how to make a backup.
#[derive(Debug)]
pub struct BackupOptions {
    /// Print filenames to the UI as they're copied.
    pub print_filenames: bool,

    /// Exclude these globs from the backup.
    pub excludes: GlobSet,
}

impl Default for BackupOptions {
    fn default() -> Self {
        BackupOptions {
            print_filenames: false,
            excludes: GlobSet::empty(),
        }
    }
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

    fn push_entry(&mut self, index_entry: IndexEntry) -> Result<()> {
        // TODO: Return or accumulate index sizes.
        self.index_builder.push_entry(index_entry)?;
        Ok(())
    }
}

impl tree::WriteTree for BackupWriter {
    fn finish(self) -> Result<CopyStats> {
        let index_builder_stats = self.index_builder.finish()?;
        self.band.close()?;
        Ok(CopyStats {
            index_builder_stats,
            ..CopyStats::default()
        })
    }

    fn copy_dir<E: Entry>(&mut self, source_entry: &E) -> Result<()> {
        // TODO: Pass back index sizes
        self.push_entry(IndexEntry::metadata_from(source_entry))
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
                self.push_entry(basis_entry)?;
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
        self.push_entry(IndexEntry {
            addrs,
            ..IndexEntry::metadata_from(source_entry)
        })?;
        Ok(stats)
    }

    fn copy_symlink<E: Entry>(&mut self, source_entry: &E) -> Result<()> {
        let target = source_entry.symlink_target().clone();
        assert!(target.is_some());
        self.push_entry(IndexEntry::metadata_from(source_entry))
    }
}
