// Conserve backup system.
// Copyright 2018, 2020, 2021 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Text output formats for structured data.
//!
//! These are objects that accept iterators of different types of content, and write it to a
//! file (typically stdout).

use std::borrow::Cow;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use conserve::archive::ValidateArchiveProblem;
use conserve::stats::Sizes;
use conserve::ui::duration_to_hms;
use conserve::{
    bytes_to_human_mb, Archive, Band, BandProblem, BandSelectionPolicy, BlockMissingReason,
    DiffEntry, DiffKind, Entry, Exclude, IndexEntry, Kind, ReadTree, Result, TreeSizeMonitor,
    ValidateMonitor, BackupMonitor, DeleteMonitor, ReferencedBlocksMonitor, RestoreMonitor
};
use nutmeg::{View, Model};
use thousands::Separable;
use tracing::{info, warn, error};

use crate::log::{self, ViewLogGuard};

/// ISO timestamp, for https://docs.rs/chrono/0.4.11/chrono/format/strftime/.
const TIMESTAMP_FORMAT: &str = "%F %T";

/// Options controlling the behavior of `show_versions`.
#[derive(Default, Clone, Eq, PartialEq)]
pub struct ShowVersionsOptions {
    /// Show versions in LIFO order by band_id.
    pub newest_first: bool,
    /// Show the total size of files in the tree.  This is
    /// slower because it requires walking the whole index.
    pub tree_size: bool,
    /// Show the date and time that each backup started.
    pub start_time: bool,
    /// Show how much time the backup took, or "incomplete" if it never finished.
    pub backup_duration: bool,
    /// Show times in UTC rather than the local timezone.
    pub utc: bool,
}

/// Prinat all available versions to the `tracing`.
pub fn show_versions(archive: &Archive, options: &ShowVersionsOptions) -> Result<()> {
    let mut band_ids = archive.list_band_ids()?;
    if options.newest_first {
        band_ids.reverse();
    }
    for band_id in band_ids {
        if !(options.tree_size || options.start_time || options.backup_duration) {
            info!("{}", band_id);
            continue;
        }
        let mut l: Vec<String> = Vec::new();
        l.push(format!("{:<20}", band_id));
        let band = match Band::open(archive, &band_id) {
            Ok(band) => band,
            Err(e) => {
                warn!("Failed to open band {:?}: {:?}", band_id, e);
                continue;
            }
        };
        let info = match band.get_info() {
            Ok(info) => info,
            Err(e) => {
                warn!("Failed to read band tail {:?}: {:?}", band_id, e);
                continue;
            }
        };

        if options.start_time {
            let start_time = info.start_time;
            let start_time_str = if options.utc {
                start_time.format(TIMESTAMP_FORMAT)
            } else {
                start_time
                    .with_timezone(&chrono::Local)
                    .format(TIMESTAMP_FORMAT)
            };
            l.push(format!("{:<10}", start_time_str));
        }

        if options.backup_duration {
            let duration_str: Cow<str> = if info.is_closed {
                info.end_time
                    .and_then(|et| (et - info.start_time).to_std().ok())
                    .map(duration_to_hms)
                    .map(Cow::Owned)
                    .unwrap_or(Cow::Borrowed("unknown"))
            } else {
                Cow::Borrowed("incomplete")
            };
            l.push(format!("{:>10}", duration_str));
        }

        if options.tree_size {
            // TODO(MH): Readd a monitor here to indicate progress
            let tree_mb_str = bytes_to_human_mb(
                archive
                    .open_stored_tree(BandSelectionPolicy::Specified(band_id.clone()))?
                    .size(Exclude::nothing(), None)?
                    .file_bytes,
            );
            l.push(format!("{:>14}", tree_mb_str,));
        }

        info!("{}", l.join(" "));
    }
    Ok(())
}

pub fn show_index_json(band: &Band) -> Result<()> {
    // TODO: Maybe use https://docs.serde.rs/serde/ser/trait.Serializer.html#method.collect_seq.
    let index_entries: Vec<IndexEntry> = band.index().iter_entries().collect();
    let json = serde_json::to_string_pretty(&index_entries)
        .map_err(|source| conserve::Error::SerializeIndex { source })?;
    for line in json.lines() {
        info!("{}", line);
    }
    Ok(())
}

pub fn show_entry_names<E: conserve::Entry, I: Iterator<Item = E>>(it: I) -> Result<()> {
    for entry in it {
        info!("{}", entry.apath());
    }
    Ok(())
}

pub fn show_diff<D: Iterator<Item = DiffEntry>>(diff: D) -> Result<()> {
    // TODO: Consider whether the actual files have changed.
    // TODO: Summarize diff.
    // TODO: Optionally include unchanged files.
    for de in diff {
        info!("{}", de);
    }

    Ok(())
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

pub struct NutmegMonitor<T: nutmeg::Model> {
    _log_guard: ViewLogGuard,
    view: Arc<Mutex<View<T>>>,
}

impl<T: nutmeg::Model + Send + 'static> NutmegMonitor<T> {
    pub fn new(initial_state: T, progress_enabled: bool) -> Self {
        // FIXME: Speed up if `progress_enabled` is false.
        //        There is no need to proxy the log output.
        //        Also updating the state can be refactored.
        let view = Arc::new(Mutex::new(nutmeg::View::new(
            initial_state,
            nutmeg::Options::default().progress_enabled(progress_enabled),
        )));

        let log_guard = log::update_terminal_target(view.clone());
        Self {
            _log_guard: log_guard,
            view,
        }
    }
}

impl<T: nutmeg::Model> NutmegMonitor<T> {
    fn locked_view(&self) -> MutexGuard<View<T>> {
        self.view.lock().expect("lock() should not fail")
    }
}

impl BackupMonitor for NutmegMonitor<BackupProgressModel> {
    fn copy(&self, entry: &conserve::LiveEntry) {
        let view = self.locked_view();
        view.update(|model| {
            model.filename = entry.apath().to_string();
            match entry.kind() {
                Kind::Dir => model.scanned_dirs += 1,
                Kind::File => model.scanned_files += 1,
                _ => (),
            }
        });
    }

    fn copy_result(&self, entry: &conserve::LiveEntry, result: &Option<conserve::DiffKind>) {
        let view = self.locked_view();
        if let Some(diff_kind) = result.as_ref() {
            view.update(|model| match diff_kind {
                &DiffKind::Changed => model.entries_changed += 1,
                &DiffKind::New => model.entries_new += 1,
                &DiffKind::Unchanged => model.entries_unchanged += 1,
                &DiffKind::Deleted => model.entries_deleted += 1,
            })
        }

        if let Some(size) = entry.size() {
            view.update(|model| model.scanned_file_bytes += size);
        }
    }

    fn copy_error(&self, entry: &conserve::LiveEntry, _error: &conserve::Error) {
        let view = self.locked_view();
        if let Some(size) = entry.size() {
            view.update(|model| model.scanned_file_bytes += size);
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
    ListBlockes {
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
            ValidateProgressState::ListBlockes { discovered } => {
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
            },
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

        let view = self.locked_view();
        view.update(|model| model.bands_total = Some(bands.len()));
    }

    fn validate_bands(&self) {
        let view = self.locked_view();
        view.update(|model| {
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
        _result: &conserve::BandValidateResult,
    ) {
        let view = self.locked_view();
        view.update(|model| {
            if let ValidateProgressState::ValidateBands { bands_done, .. } = &mut model.state {
                *bands_done += 1;
            } else {
                panic!("Expected state ValidateProgressState::ValidateBands");
            }
        });
    }

    fn validate_bands_finished(&self) {
        let mut elapsed: Option<Duration> = None;

        // We can't use logging while locked_view is held since we would deadlock.
        {
            let view = self.locked_view();
            view.update(|model| {
                if let ValidateProgressState::ValidateBands { start, .. } = &mut model.state {
                    elapsed = Some(start.elapsed());
                } else {
                    panic!("Expected state ValidateProgressState::ValidateBands");
                }
            });
        }

        info!(
            "Finished validating bands in {:#?}.",
            elapsed.expect("elapsed to be set")
        );
    }

    fn list_block_names(&self, current_count: usize) {
        if current_count == 0 {
            info!("Count blocks...");
        }

        let view = self.locked_view();
        view.update(|model| {
            model.state = ValidateProgressState::ListBlockes {
                discovered: current_count,
            }
        });
    }

    fn read_blocks(&self, count: usize) {
        info!("Check {} blocks...", count.separate_with_commas());

        let view = self.locked_view();
        view.update(|model| {
            model.state = ValidateProgressState::ReadBlocks {
                total_blocks: count,
                blocks_done: 0,
                bytes_done: 0,
                start: Instant::now(),
            }
        });
    }

    fn read_block_result(
        &self,
        _block_hash: &conserve::BlockHash,
        result: &Result<(Vec<u8>, Sizes)>,
    ) {
        let view = self.locked_view();

        view.update(|model| {
            if let ValidateProgressState::ReadBlocks {
                blocks_done,
                bytes_done,
                ..
            } = &mut model.state
            {
                if let Ok((bytes, _sizes)) = result {
                    *bytes_done += bytes.len();
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
        let view = self.locked_view();
        view.update(|model| {
            model.files += 1;
            model.total_bytes += size.unwrap_or(0);
        });
    }
}

pub enum DeleteProcessState {
    ListReferencedBlocks {
        count: usize,
    },
    FindPresentBlocks {
        count: usize,
    },
    MeasureUnreferencedBlocks {
        count: usize,
        target: usize,
    },
    DeleteBands {
        count: usize,
        target: usize,
    },
    DeleteBlocks {
        count: usize,
        target: usize,
    }
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
            },
            DeleteProcessState::FindPresentBlocks { count } => {
                format!("Find present blocks ({} discovered)", count)
            },
            DeleteProcessState::MeasureUnreferencedBlocks { count, target } => {
                format!("Measure unreferenced blocks ({}/{})", count, target)
            },
            DeleteProcessState::DeleteBands { count, target } => {
                format!("Delete bands ({}/{})", count, target)
            },
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
        let view = self.locked_view();
        view.update(|view| {
            *view = DeleteProcessState::FindPresentBlocks { count: current_count };
        });
    }

    fn measure_unreferenced_blocks(&self, current_count: usize, target_count: usize) {
        let view = self.locked_view();
        view.update(|view| {
            *view = DeleteProcessState::MeasureUnreferencedBlocks { count: current_count, target: target_count };
        });
    }

    fn delete_bands(&self, current_count: usize, target_count: usize) {
        let view = self.locked_view();
        view.update(|view| {
            *view = DeleteProcessState::DeleteBands { count: current_count, target: target_count };
        });
    }

    fn delete_blocks(&self, current_count: usize, target_count: usize) {
        let view = self.locked_view();
        view.update(|view| {
            *view = DeleteProcessState::DeleteBlocks { count: current_count, target: target_count };
        });
    }
}

impl ReferencedBlocksMonitor for NutmegMonitor<DeleteProcessState> {
    fn list_referenced_blocks(&self, current_count: usize) {
        let view = self.locked_view();
        view.update(|view| {
            *view = DeleteProcessState::ListReferencedBlocks { count: current_count };
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
            bytes_done: 0
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
        let mut print_filename = false;
        {
            let view = self.locked_view();
            view.update(|view| {
                print_filename = view.print_filenames;
                view.filename = entry.apath().to_string();
            });
        }

        if print_filename {
            info!("{}", entry.apath());
        }
    }

    fn restore_entry_result(&self, entry: &IndexEntry, _result: &Result<()>) {
        if let Some(bytes) = entry.size() {
            let view = self.locked_view();
            view.update(|view| view.bytes_done += bytes);
        }
    }
}