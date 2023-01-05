use crate::{
    archive::ValidateArchiveProblem, stats::Sizes, validate::BlockLengths, BackupStats, Band,
    BandId, BandProblem, BandValidateError, BlockHash, BlockMissingReason, DiffKind, Error,
    IndexEntry, LiveEntry, ReadTree, Result, ValidateStats,
};

/// Monitor the backup progress.
pub trait BackupMonitor {
    fn copy(&self, _entry: &LiveEntry) {}
    fn copy_error(&self, _entry: &LiveEntry, _error: &Error) {}
    fn copy_result(&self, _entry: &LiveEntry, _result: &Option<DiffKind>) {}

    fn finished(&self, _stats: &BackupStats) {}
}

#[derive(Debug, Clone)]
pub enum ValidateProgress {
    CountBands,
    CountBandsFinished,

    ValidateArchive,
    ValidateArchiveFinished,

    ValidateBands { current: usize, total: usize },
    ValidateBandsFinished { total: usize },

    ListBlockNames { discovered: usize },
    ListBlockNamesFinished { total: usize },

    BlockRead { current: usize, total: usize },
    BlockReadFinished { total: usize },

    ValidateBlocks,
    ValidateBlocksFinished,
}

/// Monitor the validation progress.
pub trait ValidateMonitor: Sync {
    /// Will be called with the current state of validating the target archive.
    fn progress(&self, _state: ValidateProgress) {}

    fn discovered_bands(&self, _bands: &[BandId]) {}

    fn archive_problem(&self, _problem: &ValidateArchiveProblem) {}
    fn band_problem(&self, _band: &Band, _problem: &BandProblem) {}
    fn band_validate_result(
        &self,
        _band_id: &BandId,
        _result: &std::result::Result<(BlockLengths, ValidateStats), BandValidateError>,
    ) {
    }
    fn block_missing(&self, _block_hash: &BlockHash, _reason: &BlockMissingReason) {}
    fn block_read_result(&self, _block_hash: &BlockHash, _result: &Result<Sizes>) {}
}

/// Monitor for iterating trees.
pub trait TreeSizeMonitor<T: ReadTree> {
    fn entry_discovered(&self, _entry: &T::Entry, _size: &Option<u64>) {}
}

#[derive(Debug, Clone)]
pub enum ReferencedBlocksProgress {
    ReferencedBlocks { discovered: usize },
    ReferencedBlocksFinished { total: usize },
}

/// Monitor for iterating referenced blocks.
pub trait ReferencedBlocksMonitor: Sync {
    fn progress(&self, _state: ReferencedBlocksProgress) {}
}

#[derive(Debug, Clone)]
pub enum DeleteProgress {
    FindPresentBlocks { discovered: usize },
    FindPresentBlocksFinished { total: usize },

    MeasureUnreferencedBlocks { current: usize, total: usize },
    MeasureUnreferencedBlocksFinished { total: usize },

    DeleteBands { current: usize, total: usize },
    DeleteBandsFinished { total: usize },

    DeleteBlocks { current: usize, total: usize },
    DeleteBlocksFinished { total: usize },
}

/// Monitor for deleting backups/blocks.
pub trait DeleteMonitor: Sync {
    fn referenced_blocks_monitor(&self) -> &dyn ReferencedBlocksMonitor;

    fn progress(&self, _state: DeleteProgress) {}
    // TODO: May encountered delete errors?
}

/// Monitor the progress of restoring files.
pub trait RestoreMonitor {
    fn restore_entry(&self, _entry: &IndexEntry) {}
    fn restore_entry_result(&self, _entry: &IndexEntry, _result: &Result<()>) {}
}

/// Default monitor which does nothing.
/// Will be used when no monitor has been specified by the caller.
pub(crate) struct NullMonitor {}
pub(crate) const NULL_MONITOR: NullMonitor = NullMonitor {};

impl BackupMonitor for NullMonitor {}
impl ValidateMonitor for NullMonitor {}
impl<T: ReadTree> TreeSizeMonitor<T> for NullMonitor {}
impl ReferencedBlocksMonitor for NullMonitor {}
impl DeleteMonitor for NullMonitor {
    fn referenced_blocks_monitor(&self) -> &dyn ReferencedBlocksMonitor {
        self
    }
}
impl RestoreMonitor for NullMonitor {}
