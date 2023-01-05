use std::sync::{Arc, Mutex};
use std::time::Instant;

use conserve::archive::ValidateArchiveProblem;
use conserve::stats::Sizes;
use conserve::{
    BackupMonitor, Band, BandProblem, BandValidateError, BlockLengths, BlockMissingReason,
    DeleteMonitor, DeleteProgress, DiffKind, Entry, IndexEntry, Kind, ReadTree,
    ReferencedBlocksMonitor, ReferencedBlocksProgress, RestoreMonitor, Result, TreeSizeMonitor,
    ValidateMonitor, ValidateProgress, ValidateStats,
};
use nutmeg::{Model, View};
use thousands::Separable;
use tracing::{debug, error, info, warn};

use crate::log::{self, ViewLogGuard};

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub enum FileListVerbosity {
    /// Do not print a file list
    None,

    /// Only print the files name
    NameOnly,

    /// Print the file name alonw with owner and permissions
    Full,
}

impl Default for FileListVerbosity {
    fn default() -> Self {
        FileListVerbosity::None
    }
}

enum NutmegMonitorState<T: nutmeg::Model> {
    NutmegProgress {
        view: Arc<Mutex<View<T>>>,
        _log_guard: ViewLogGuard,
    },
    NoProgress {
        state: Mutex<T>,
    },
}

pub struct NutmegMonitor<T: nutmeg::Model> {
    state: NutmegMonitorState<T>,
}

impl<T: nutmeg::Model + Send + 'static> NutmegMonitor<T> {
    pub fn new(initial_state: T, progress_enabled: bool) -> Self {
        let state = if progress_enabled {
            let view = Arc::new(Mutex::new(nutmeg::View::new(
                initial_state,
                nutmeg::Options::default().progress_enabled(progress_enabled),
            )));

            let log_guard = log::update_terminal_target(view.clone());
            NutmegMonitorState::NutmegProgress {
                view,
                _log_guard: log_guard,
            }
        } else {
            NutmegMonitorState::NoProgress {
                state: Mutex::new(initial_state),
            }
        };

        Self { state }
    }

    fn update_model<F: FnOnce(&mut T) -> R, R>(&self, update_fn: F) -> R {
        match &self.state {
            NutmegMonitorState::NutmegProgress { view, .. } => {
                let view = view.lock().expect("lock() should not fail");
                view.update(update_fn)
            }
            NutmegMonitorState::NoProgress { state } => {
                let mut state = state.lock().expect("lock() should not fail");
                update_fn(&mut *state)
            }
        }
    }

    fn inspect_model<F: FnOnce(&mut T) -> R, R>(&self, inspect_fn: F) -> R {
        match &self.state {
            NutmegMonitorState::NutmegProgress { view, .. } => {
                let view = view.lock().expect("lock() should not fail");
                view.inspect_model(inspect_fn)
            }
            NutmegMonitorState::NoProgress { state } => {
                let mut state = state.lock().expect("lock() should not fail");
                inspect_fn(&mut *state)
            }
        }
    }
}

// Considerations if we're trying properly extimate the remaining progress.
//
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

#[derive(Default)]
pub struct BackupProgressModel {
    pub file_list: FileListVerbosity,
    filename: String,

    scanned_file_bytes: u64,
    scanned_dirs: usize,
    scanned_files: usize,

    entries_new: usize,
    entries_changed: usize,
    entries_unchanged: usize,
    entries_deleted: usize,
}

impl nutmeg::Model for BackupProgressModel {
    fn render(&mut self, _width: usize) -> String {
        format!(
            "Scanned {} directories, {} files, {} MB\n{} new entries, {} changed, {} deleted, {} unchanged\n{}",
            self.scanned_dirs,
            self.scanned_files,
            self.scanned_file_bytes / 1_000_000,
            self.entries_new, self.entries_changed, self.entries_deleted, self.entries_unchanged,
            self.filename
        )
    }
}

impl BackupMonitor for NutmegMonitor<BackupProgressModel> {
    fn copy(&self, entry: &conserve::LiveEntry) {
        self.update_model(|model| {
            model.filename = entry.apath().to_string();
            match entry.kind() {
                Kind::Dir => model.scanned_dirs += 1,
                Kind::File => model.scanned_files += 1,
                _ => (),
            }
        });
    }

    fn copy_result(&self, entry: &conserve::LiveEntry, result: &Option<conserve::DiffKind>) {
        if let Some(diff_kind) = result.as_ref() {
            let file_list = self.update_model(|model| {
                match diff_kind {
                    DiffKind::Changed => model.entries_changed += 1,
                    DiffKind::New => model.entries_new += 1,
                    DiffKind::Unchanged => model.entries_unchanged += 1,
                    DiffKind::Deleted => model.entries_deleted += 1,
                };

                model.file_list
            });

            match file_list {
                FileListVerbosity::None => {}
                FileListVerbosity::NameOnly => info!("{} {}", diff_kind.as_sigil(), entry.apath()),
                FileListVerbosity::Full => {
                    info!(
                        "{} {} {} {}",
                        diff_kind.as_sigil(),
                        entry.unix_mode(),
                        entry.owner(),
                        entry.apath()
                    );
                }
            }
        }

        if let Some(size) = entry.size() {
            self.update_model(|model| model.scanned_file_bytes += size);
        }
    }

    fn copy_error(&self, entry: &conserve::LiveEntry, _error: &conserve::Error) {
        if let Some(size) = entry.size() {
            self.update_model(|model| model.scanned_file_bytes += size);
        }
    }
}

#[derive(Debug, Default)]
pub struct ValidateProgressModel {
    progress: Option<ValidateProgress>,

    bands_validated: usize,
    bands_start: Option<Instant>,

    read_blocks_count: usize,
    read_blocks_bytes: usize,
}

impl nutmeg::Model for ValidateProgressModel {
    fn render(&mut self, _width: usize) -> String {
        let state = match &self.progress {
            Some(state) => state,
            None => return "Validating, please wait...".to_string(),
        };

        match state {
            ValidateProgress::CountBands => "Counting bands".to_string(),
            ValidateProgress::CountBandsFinished => "Finished counting bands".to_string(),

            ValidateProgress::ValidateArchive => "Validating archive integrity".to_string(),
            ValidateProgress::ValidateArchiveFinished => {
                "Finished validating archive integrity".to_string()
            }

            ValidateProgress::ValidateBlocks => format!("Validating blocks"),
            ValidateProgress::ValidateBlocksFinished => format!("Blocks validated"),

            ValidateProgress::ValidateBands { current, total } => {
                format!("Validating band {}/{}", current, total)
            }
            ValidateProgress::ValidateBandsFinished { total } => {
                format!("{} bands validated", total)
            }

            ValidateProgress::ListBlockNames { discovered } => {
                format!("Listing blocks ({} blocks discovered)", discovered)
            }
            ValidateProgress::ListBlockNamesFinished { total } => {
                format!("Discovered {} blocks", total)
            }

            ValidateProgress::BlockRead { total, .. } => {
                // Note: We're using our own read block counter (`read_blocks_count`) since the current argument in ValidateProgress::BlockRead
                //       is not garanteed to be in sequential due to multithreading.
                let start = self.bands_start.get_or_insert_with(|| Instant::now());
                format!(
                    "Check block {}/{}: {} done, {} MB checked, {} remaining",
                    self.read_blocks_count,
                    total,
                    nutmeg::percent_done(self.read_blocks_count, *total),
                    self.read_blocks_bytes / 1_000_000,
                    nutmeg::estimate_remaining(start, self.read_blocks_count, *total)
                )
            }
            ValidateProgress::BlockReadFinished { total } => {
                format!("Finished reading {} blocks", total)
            }
        }
    }
}

impl ValidateMonitor for NutmegMonitor<ValidateProgressModel> {
    fn progress(&self, state: ValidateProgress) {
        match &state {
            ValidateProgress::CountBands => info!("Count bands..."),
            ValidateProgress::ValidateArchive => info!("Check archive top-level directory..."),
            ValidateProgress::ListBlockNames { discovered } => {
                if *discovered == 0 {
                    info!("Count blocks...");
                }
            }
            ValidateProgress::ValidateBands { .. } => {
                self.update_model(|state| {
                    if state.bands_start.is_none() {
                        state.bands_start = Some(Instant::now());
                    }
                });
            }
            ValidateProgress::ValidateBandsFinished { .. } => {
                let start = self.inspect_model(|state| state.bands_start.unwrap_or(Instant::now()));
                info!("Finished validating bands in {:#?}.", start.elapsed());
            }
            ValidateProgress::BlockRead { current, .. } => {
                if *current == 0 {
                    info!("Check {} blocks...", current.separate_with_commas());
                }
            }
            _ => {}
        }

        debug!("{:?}", &state);
        self.update_model(|model| model.progress = Some(state));
    }

    fn archive_problem(&self, problem: &ValidateArchiveProblem) {
        match problem {
            ValidateArchiveProblem::UnexpectedFileType { name, kind } => {
                error!(
                    "Unexpected file kind in archive directory: {:?} of kind {:?}",
                    name, kind
                );
            }
            ValidateArchiveProblem::DirectoryListError { error } => {
                error!("Error listing archive directory: {:?}", error);
            }
            ValidateArchiveProblem::UnexpectedFiles { path, files } => {
                error!(
                    "Unexpected files in archive directory {:?}: {:?}",
                    path, files
                );
            }
            ValidateArchiveProblem::DuplicateBand { path, directory } => {
                error!("Duplicated band directory in {:?}: {:?}", path, directory);
            }
            ValidateArchiveProblem::UnexpectedDirectory { path, directory } => {
                error!("Unexpected directory in {:?}: {:?}", path, directory);
            }
        }
    }

    fn discovered_bands(&self, bands: &[conserve::BandId]) {
        info!("Checking {} bands...", bands.len());
    }

    fn band_problem(&self, band: &Band, problem: &conserve::BandProblem) {
        match problem {
            BandProblem::MissingHeadFile { .. } => {
                warn!("No band head file in {:?}", band.transport())
            }
            BandProblem::UnexpectedFiles { files } => warn!(
                "Unexpected files in band directory {:?}: {:?}",
                band.transport(),
                files
            ),
            BandProblem::UnexpectedDirectories { directories } => warn!(
                "Incongruous directories in band directory {:?}: {:?}",
                band.transport(),
                directories
            ),
        }
    }

    fn band_validate_result(
        &self,
        _band_id: &conserve::BandId,
        _result: &std::result::Result<(BlockLengths, ValidateStats), BandValidateError>,
    ) {
        self.update_model(|model| {
            model.bands_validated += 1;
        });
    }

    fn block_read_result(&self, _block_hash: &conserve::BlockHash, result: &Result<Sizes>) {
        self.update_model(|model| {
            model.read_blocks_count += 1;
            if let Ok(sizes) = result {
                model.read_blocks_bytes += sizes.uncompressed as usize;
            } else {
                // TODO: Add a fail counter.
            }
        });
    }

    fn block_missing(
        &self,
        block_hash: &conserve::BlockHash,
        reason: &conserve::BlockMissingReason,
    ) {
        match reason {
            BlockMissingReason::NotExisting => warn!("Block {:?} is missing", block_hash),
            BlockMissingReason::InvalidRange => warn!("Block {:?} is too short", block_hash),
        }
    }
}

#[derive(Default)]
pub struct SizeProgressModel {
    files: usize,
    total_bytes: u64,
}
impl nutmeg::Model for SizeProgressModel {
    fn render(&mut self, _width: usize) -> String {
        format!(
            "Measuring... {} files, {} MB",
            self.files,
            self.total_bytes / 1_000_000
        )
    }
}

impl<T: ReadTree> TreeSizeMonitor<T> for NutmegMonitor<SizeProgressModel> {
    fn entry_discovered(&self, _entry: &<T as ReadTree>::Entry, size: &Option<u64>) {
        self.update_model(|model| {
            model.files += 1;
            model.total_bytes += size.unwrap_or(0);
        });
    }
}

// ReferencedBlocksProgress

pub enum DeleteProcessModel {
    Unset,
    List(ReferencedBlocksProgress),
    Delete(DeleteProgress),
}

impl Default for DeleteProcessModel {
    fn default() -> Self {
        Self::Unset
    }
}

impl Model for DeleteProcessModel {
    fn render(&mut self, _width: usize) -> String {
        match self {
            Self::List(state) => match state {
                ReferencedBlocksProgress::ReferencedBlocks { discovered } => {
                    format!("Find referenced blocks in band ({} discovered)", discovered)
                }
                ReferencedBlocksProgress::ReferencedBlocksFinished { total } => {
                    format!("Discovered {} referenced blocks in band", total)
                }
            },
            Self::Delete(state) => match state {
                DeleteProgress::FindPresentBlocks { discovered } => {
                    format!("Find present blocks ({} discovered)", discovered)
                }
                DeleteProgress::FindPresentBlocksFinished { total } => {
                    format!("Found {} present blocks", total)
                }

                DeleteProgress::MeasureUnreferencedBlocks { current, total } => {
                    format!("Measure unreferenced blocks ({}/{})", current, total)
                }
                DeleteProgress::MeasureUnreferencedBlocksFinished { .. } => {
                    format!("Measured unreferenced blocks")
                }

                DeleteProgress::DeleteBands { current, total } => {
                    format!("Delete bands ({}/{})", current, total)
                }
                DeleteProgress::DeleteBandsFinished { total } => format!("Deleted {} bands", total),

                DeleteProgress::DeleteBlocks { current, total } => {
                    format!("Delete blocks ({}/{})", current, total)
                }
                DeleteProgress::DeleteBlocksFinished { total } => {
                    format!("Deleted {} blocks", total)
                }
            },
            Self::Unset => "Deleting, please wait...".to_string(),
        }
    }
}

impl DeleteMonitor for NutmegMonitor<DeleteProcessModel> {
    fn referenced_blocks_monitor(&self) -> &dyn conserve::ReferencedBlocksMonitor {
        self
    }
    fn progress(&self, state: DeleteProgress) {
        self.update_model(|model| *model = DeleteProcessModel::Delete(state));
    }
}

impl ReferencedBlocksMonitor for NutmegMonitor<DeleteProcessModel> {
    fn progress(&self, state: ReferencedBlocksProgress) {
        self.update_model(|model| *model = DeleteProcessModel::List(state));
    }
}

pub struct RestoreProgressModel {
    file_list: FileListVerbosity,
    filename: String,
    bytes_done: u64,
}

impl RestoreProgressModel {
    pub fn new(file_list: FileListVerbosity) -> Self {
        Self {
            file_list,
            filename: "".to_string(),
            bytes_done: 0,
        }
    }
}

impl nutmeg::Model for RestoreProgressModel {
    fn render(&mut self, _width: usize) -> String {
        format!(
            "Restoring: {} MB\n{}",
            self.bytes_done / 1_000_000,
            self.filename
        )
    }
}

impl RestoreMonitor for NutmegMonitor<RestoreProgressModel> {
    fn restore_entry(&self, entry: &IndexEntry) {
        let file_list = self.update_model(|view| {
            view.filename = entry.apath().to_string();
            view.file_list
        });

        match file_list {
            FileListVerbosity::None => {}
            FileListVerbosity::NameOnly => info!("{}", entry.apath()),
            FileListVerbosity::Full => {
                info!(
                    "{} {} {}",
                    entry.unix_mode(),
                    entry.owner(),
                    entry.apath()
                );
            }
        }
    }

    fn restore_entry_result(&self, entry: &IndexEntry, _result: &Result<()>) {
        if let Some(bytes) = entry.size() {
            self.update_model(|view| view.bytes_done += bytes);
        }
    }
}

#[derive(Default)]
pub struct ReferencedBlocksProgressModel {
    progress: Option<ReferencedBlocksProgress>,
}

impl Model for ReferencedBlocksProgressModel {
    fn render(&mut self, _width: usize) -> String {
        let state = match &self.progress {
            Some(state) => state,
            None => return "Listing referenced blocks, please wait...".to_string(),
        };

        match state {
            ReferencedBlocksProgress::ReferencedBlocks { discovered } => {
                format!("Find referenced blocks in band ({} discovered)", discovered)
            }
            ReferencedBlocksProgress::ReferencedBlocksFinished { total } => {
                format!("Discovered {} referenced blocks in band", total)
            }
        }
    }
}

impl ReferencedBlocksMonitor for NutmegMonitor<ReferencedBlocksProgressModel> {
    fn progress(&self, state: ReferencedBlocksProgress) {
        self.update_model(|model| model.progress = Some(state));
    }
}
