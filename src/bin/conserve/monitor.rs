use std::sync::{Arc, Mutex};
use std::time::Instant;

use conserve::archive::ValidateArchiveProblem;
use conserve::stats::Sizes;
use conserve::{
    BackupMonitor, Band, BandProblem, BandValidateError, BlockLengths, BlockMissingReason,
    DeleteMonitor, DiffKind, Entry, IndexEntry, Kind, ReadTree, ReferencedBlocksMonitor,
    RestoreMonitor, Result, TreeSizeMonitor, ValidateMonitor, ValidateStats,
};
use nutmeg::{Model, View};
use thousands::Separable;
use tracing::{error, info, warn};

use crate::log::{self, ViewLogGuard};

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
    pub verbose: bool,
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
            let verbose = self.update_model(|model| {
                match diff_kind {
                    DiffKind::Changed => model.entries_changed += 1,
                    DiffKind::New => model.entries_new += 1,
                    DiffKind::Unchanged => model.entries_unchanged += 1,
                    DiffKind::Deleted => model.entries_deleted += 1,
                };

                model.verbose
            });

            if verbose {
                info!("{} {}", diff_kind.as_sigil(), entry.apath());
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

enum ValidateProgressState {
    CountBands,
    ValidateBands {
        bands_done: usize,
        bands_total: usize,
        start: Instant,
    },
    ListBlocks {
        discovered: usize,
    },
    ReadBlocks {
        total_blocks: usize,
        blocks_done: usize,
        bytes_done: usize,
        start: Instant,
    },
}

pub struct ValidateProgressModel {
    bands_total: Option<usize>,
    state: ValidateProgressState,
}

impl Default for ValidateProgressModel {
    fn default() -> Self {
        Self {
            bands_total: None,
            state: ValidateProgressState::CountBands {},
        }
    }
}

impl nutmeg::Model for ValidateProgressModel {
    fn render(&mut self, _width: usize) -> String {
        match &self.state {
            ValidateProgressState::CountBands => "Counting bands".to_string(),
            ValidateProgressState::ValidateBands {
                bands_done,
                bands_total,
                start,
            } => {
                format!(
                    "Check index {}/{}, {} done, {} remaining",
                    bands_done,
                    bands_total,
                    nutmeg::percent_done(*bands_done, *bands_total),
                    nutmeg::estimate_remaining(start, *bands_done, *bands_total)
                )
            }
            ValidateProgressState::ListBlocks { discovered } => {
                format!("Listing blocks ({} blocks discovered)", discovered)
            }
            ValidateProgressState::ReadBlocks {
                total_blocks,
                blocks_done,
                bytes_done,
                start,
            } => {
                format!(
                    "Check block {}/{}: {} done, {} MB checked, {} remaining",
                    *blocks_done,
                    *total_blocks,
                    nutmeg::percent_done(*blocks_done, *total_blocks),
                    *bytes_done / 1_000_000,
                    nutmeg::estimate_remaining(start, *blocks_done, *total_blocks)
                )
            }
        }
    }
}

impl ValidateMonitor for NutmegMonitor<ValidateProgressModel> {
    fn validate_archive(&self) {
        info!("Check archive top-level directory...");
    }

    fn validate_archive_problem(&self, problem: &ValidateArchiveProblem) {
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

    fn count_bands(&self) {
        info!("Count bands...");
    }

    fn count_bands_result(&self, bands: &[conserve::BandId]) {
        info!("Checking {} bands...", bands.len());

        self.update_model(|model| model.bands_total = Some(bands.len()));
    }

    fn validate_bands(&self) {
        self.update_model(|model| {
            let bands_total = model.bands_total.expect("bands have been counted");
            model.state = ValidateProgressState::ValidateBands {
                bands_done: 0,
                bands_total,
                start: Instant::now(),
            };
        });
    }

    fn validate_band_problem(&self, band: &Band, problem: &conserve::BandProblem) {
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

    fn validate_band_result(
        &self,
        _band_id: &conserve::BandId,
        _result: &std::result::Result<(BlockLengths, ValidateStats), BandValidateError>,
    ) {
        self.update_model(|model| {
            if let ValidateProgressState::ValidateBands { bands_done, .. } = &mut model.state {
                *bands_done += 1;
            } else {
                panic!("Expected state ValidateProgressState::ValidateBands");
            }
        });
    }

    fn validate_bands_finished(&self) {
        // We can't use logging while locked_view is held since we would deadlock.
        let elapsed = self.update_model(|model| {
            if let ValidateProgressState::ValidateBands { start, .. } = &mut model.state {
                start.elapsed()
            } else {
                panic!("Expected state ValidateProgressState::ValidateBands");
            }
        });

        info!("Finished validating bands in {:#?}.", elapsed);
    }

    fn list_block_names(&self, current_count: usize) {
        if current_count == 0 {
            info!("Count blocks...");
        }

        self.update_model(|model| {
            model.state = ValidateProgressState::ListBlocks {
                discovered: current_count,
            }
        });
    }

    fn read_blocks(&self, count: usize) {
        info!("Check {} blocks...", count.separate_with_commas());

        self.update_model(|model| {
            model.state = ValidateProgressState::ReadBlocks {
                total_blocks: count,
                blocks_done: 0,
                bytes_done: 0,
                start: Instant::now(),
            }
        });
    }

    fn read_block_result(&self, _block_hash: &conserve::BlockHash, result: &Result<Sizes>) {
        self.update_model(|model| {
            if let ValidateProgressState::ReadBlocks {
                blocks_done,
                bytes_done,
                ..
            } = &mut model.state
            {
                if let Ok(sizes) = result {
                    *bytes_done += sizes.uncompressed as usize;
                } else {
                    // TODO: Add a fail counter.
                }

                *blocks_done += 1;
            } else {
                panic!("Expected state ValidateProgressState::ReadBlocks");
            }
        });
    }

    fn validate_block_missing(
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

pub enum DeleteProcessState {
    ListReferencedBlocks { count: usize },
    FindPresentBlocks { count: usize },
    MeasureUnreferencedBlocks { count: usize, target: usize },
    DeleteBands { count: usize, target: usize },
    DeleteBlocks { count: usize, target: usize },
}

impl Default for DeleteProcessState {
    fn default() -> Self {
        DeleteProcessState::ListReferencedBlocks { count: 0 }
    }
}

impl Model for DeleteProcessState {
    fn render(&mut self, _width: usize) -> String {
        match self {
            DeleteProcessState::ListReferencedBlocks { count } => {
                format!("Find referenced blocks in band ({} discovered)", count)
            }
            DeleteProcessState::FindPresentBlocks { count } => {
                format!("Find present blocks ({} discovered)", count)
            }
            DeleteProcessState::MeasureUnreferencedBlocks { count, target } => {
                format!("Measure unreferenced blocks ({}/{})", count, target)
            }
            DeleteProcessState::DeleteBands { count, target } => {
                format!("Delete bands ({}/{})", count, target)
            }
            DeleteProcessState::DeleteBlocks { count, target } => {
                format!("Delete blocks ({}/{})", count, target)
            }
        }
    }
}

impl DeleteMonitor for NutmegMonitor<DeleteProcessState> {
    fn referenced_blocks_monitor(&self) -> &dyn conserve::ReferencedBlocksMonitor {
        self
    }

    fn find_present_blocks(&self, current_count: usize) {
        self.update_model(|view| {
            *view = DeleteProcessState::FindPresentBlocks {
                count: current_count,
            };
        });
    }

    fn measure_unreferenced_blocks(&self, current_count: usize, target_count: usize) {
        self.update_model(|view| {
            *view = DeleteProcessState::MeasureUnreferencedBlocks {
                count: current_count,
                target: target_count,
            };
        });
    }

    fn delete_bands(&self, current_count: usize, target_count: usize) {
        self.update_model(|view| {
            *view = DeleteProcessState::DeleteBands {
                count: current_count,
                target: target_count,
            };
        });
    }

    fn delete_blocks(&self, current_count: usize, target_count: usize) {
        self.update_model(|view| {
            *view = DeleteProcessState::DeleteBlocks {
                count: current_count,
                target: target_count,
            };
        });
    }
}

impl ReferencedBlocksMonitor for NutmegMonitor<DeleteProcessState> {
    fn list_referenced_blocks(&self, current_count: usize) {
        self.update_model(|view| {
            *view = DeleteProcessState::ListReferencedBlocks {
                count: current_count,
            };
        });
    }
}

pub struct RestoreProgressModel {
    print_filenames: bool,
    filename: String,
    bytes_done: u64,
}

impl RestoreProgressModel {
    pub fn new(print_filenames: bool) -> Self {
        Self {
            print_filenames,
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
        let print_filename = self.update_model(|view| {
            view.filename = entry.apath().to_string();
            view.print_filenames
        });

        if print_filename {
            info!("{}", entry.apath());
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
    count: usize,
}

impl Model for ReferencedBlocksProgressModel {
    fn render(&mut self, _width: usize) -> String {
        format!("Find referenced blocks in band ({} discovered)", self.count)
    }
}

impl ReferencedBlocksMonitor for NutmegMonitor<ReferencedBlocksProgressModel> {
    fn list_referenced_blocks(&self, current_count: usize) {
        self.update_model(|model| model.count = current_count);
    }
}
