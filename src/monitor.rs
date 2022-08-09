use crate::{ReadTree, LiveEntry, Error, DiffKind, BackupStats, BandId, BandProblem, BandValidateResult, BlockMissingReason, BlockHash, Band, stats::Sizes, Result, archive::ValidateArchiveProblem, IndexEntry};

/// Monitor the backup progress.
pub trait BackupMonitor {
    /// Will be called before the entry will be backupped
    fn copy(&self, _entry: &LiveEntry) {}
    fn copy_error(&self, _entry: &LiveEntry, _error: &Error) {}
    fn copy_result(&self, _entry: &LiveEntry, _result: &Option<DiffKind>) {}

    fn finished(&self, _stats: &BackupStats) {}
}

/// Monitor the validation progress.
pub trait ValidateMonitor : Sync {
    fn count_bands(&self) {}
    fn count_bands_result(&self, _bands: &[BandId]) {}

    fn validate_archive(&self) {}
    fn validate_archive_problem(&self, _problem: &ValidateArchiveProblem) {}
    fn validate_archive_finished(&self) {}

    fn validate_bands(&self) {}
    fn validate_bands_finished(&self) {}

    fn validate_band(&self, _band_id: &BandId) {}
    fn validate_band_problem(&self, _band: &Band, _problem: &BandProblem) {}
    fn validate_band_result(&self, _band_id: &BandId, _result: &BandValidateResult) {}

    fn validate_block_missing(&self, _block_hash: &BlockHash, _reason: &BlockMissingReason) {}
    fn validate_blocks(&self) {}
    fn validate_blocks_finished(&self) {}

    fn list_block_names(&self, _current_count: usize) {}
    fn list_block_names_finished(&self) {}
    
    fn read_blocks(&self, _count: usize) {}
    fn read_block_result(&self, _block_hash: &BlockHash, _result: &Result<(Vec<u8>, Sizes)>) {}
    fn read_blocks_finished(&self) {}
}

/// Monitor for iterating trees.
pub trait TreeSizeMonitor<T: ReadTree> {
    fn entry_discovered(&self, _entry: &T::Entry, _size: &Option<u64>) {}
}

pub trait ReferencedBlocksMonitor : Sync {
    fn list_referenced_blocks(&self, _current_count: usize) {}
    fn list_referenced_blocks_finished(&self) {}
}

pub trait DeleteMonitor : Sync {
    fn referenced_blocks_monitor(&self) -> &dyn ReferencedBlocksMonitor;

    fn find_present_blocks(&self, _current_count: usize) {}
    fn find_present_blocks_finished(&self) {}

    fn measure_unreferenced_blocks(&self, _current_count: usize, _target_count: usize) {}
    fn measure_unreferenced_blocks_finished(&self) {}

    fn delete_bands(&self, _current_count: usize, _target_count: usize) {}
    fn delete_bands_finished(&self) {}

    fn delete_blocks(&self, _current_count: usize, _target_count: usize) {}
    fn delete_blocks_finished(&self) {}
}

pub trait RestoreMonitor {
    fn restore_entry(&self, _entry: &IndexEntry) {}
    fn restore_entry_result(&self, _entry: &IndexEntry, _result: &Result<()>) {}
}

/// Default monitor which does nothing.
/// Will be used when no monitor has been specified by the caller.
pub(crate) struct DefaultMonitor {}

impl BackupMonitor for DefaultMonitor {}
impl ValidateMonitor for DefaultMonitor {}
impl<T: ReadTree> TreeSizeMonitor<T> for DefaultMonitor {}
impl ReferencedBlocksMonitor for DefaultMonitor {}
impl DeleteMonitor for DefaultMonitor {
    fn referenced_blocks_monitor(&self) -> &dyn ReferencedBlocksMonitor {
        self
    }
}
impl RestoreMonitor for DefaultMonitor {}