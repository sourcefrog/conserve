use crate::{ReadTree, LiveEntry, Error, DiffKind, BackupStats, BandId, BandProblem, BandValidateResult, BlockMissingReason, BlockHash, Band, stats::Sizes, Result, archive::ValidateArchiveProblem, IndexEntry};

/// Monitor the backup progress.
pub trait BackupMonitor {
    /// Will be called before the entry will be backupped
    fn copy(&mut self, _entry: &LiveEntry) {}
    fn copy_error(&mut self, _entry: &LiveEntry, _error: &Error) {}
    fn copy_result(&mut self, _entry: &LiveEntry, _result: &Option<DiffKind>) {}

    fn finished(&mut self, _stats: &BackupStats) {}
}

/// Monitor the validation progress.
pub trait ValidateMonitor {
    fn count_bands(&mut self) {}
    fn count_bands_result(&mut self, _bands: &[BandId]) {}

    fn validate_archive(&mut self) {}
    fn validate_archive_problem(&mut self, _problem: &ValidateArchiveProblem) {}
    fn validate_archive_finished(&mut self) {}

    fn validate_bands(&mut self) {}
    fn validate_bands_finished(&mut self) {}

    fn validate_band(&mut self, _band_id: &BandId) {}
    fn validate_band_problem(&mut self, _band: &Band, _problem: &BandProblem) {}
    fn validate_band_result(&mut self, _band_id: &BandId, _result: &BandValidateResult) {}

    fn validate_block_missing(&mut self, _block_hash: &BlockHash, _reason: &BlockMissingReason) {}
    fn validate_blocks(&mut self) {}
    fn validate_blocks_finished(&mut self) {}

    fn list_block_names(&mut self, _current_count: usize) {}
    fn list_block_names_finished(&mut self) {}
    
    fn read_blocks(&mut self, _count: usize) {}
    fn read_block_result(&mut self, _block_hash: &BlockHash, _result: &Result<(Vec<u8>, Sizes)>) {}
    fn read_blocks_finished(&mut self) {}
}

/// Monitor for iterating trees.
pub trait TreeSizeMonitor<T: ReadTree> {
    fn entry_discovered(&mut self, _entry: &T::Entry, _size: &Option<u64>) {}
}

pub trait ReferencedBlocksMonitor {
    fn list_referenced_blocks(&mut self, _current_count: usize) {}
    fn list_referenced_blocks_finished(&mut self) {}
}

pub trait DeleteMonitor {
    fn referenced_blocks_monitor(&mut self) -> &mut dyn ReferencedBlocksMonitor;

    fn find_present_blocks(&mut self, _current_count: usize) {}
    fn find_present_blocks_finished(&mut self) {}

    fn measure_unreferenced_blocks(&mut self, _current_count: usize, _target_count: usize) {}
    fn measure_unreferenced_blocks_finished(&mut self) {}

    fn delete_bands(&mut self, _current_count: usize, _target_count: usize) {}
    fn delete_bands_finished(&mut self) {}

    fn delete_blocks(&mut self, _current_count: usize, _target_count: usize) {}
    fn delete_blocks_finished(&mut self) {}
}

pub trait RestoreMonitor {
    fn restore_entry(&mut self, _entry: &IndexEntry) {}
    fn restore_entry_result(&mut self, _entry: &IndexEntry, _result: &Result<()>) {}
}

/// Default monitor which does nothing.
/// Will be used when no monitor has been specified by the caller.
pub(crate) struct DefaultMonitor {}

impl BackupMonitor for DefaultMonitor {}
impl ValidateMonitor for DefaultMonitor {}
impl<T: ReadTree> TreeSizeMonitor<T> for DefaultMonitor {}
impl ReferencedBlocksMonitor for DefaultMonitor {}
impl DeleteMonitor for DefaultMonitor {
    fn referenced_blocks_monitor(&mut self) -> &mut dyn ReferencedBlocksMonitor {
        self
    }
}
impl RestoreMonitor for DefaultMonitor {}