use crate::{ReadTree, LiveEntry, Error, DiffKind, BackupStats, BandId, BandProblem, BandValidateResult, BlockMissingReason, BlockHash, Band, stats::Sizes, Result};

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

/// Default monitor which does nothing.
/// Will be used when no monitor has been specified by the caller.
pub(crate) struct DefaultMonitor {}

impl BackupMonitor for DefaultMonitor {}
impl ValidateMonitor for DefaultMonitor {}
impl<T: ReadTree> TreeSizeMonitor<T> for DefaultMonitor { }