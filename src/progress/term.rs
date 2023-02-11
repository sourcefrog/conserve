// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use std::io;

use itertools::Itertools;
use lazy_static::lazy_static;
use nutmeg::estimate_remaining;
use thousands::Separable;

use super::*;

lazy_static! {
    /// A global Nutmeg view.
    ///
    /// This is global to reflect that there is globally one stdout/stderr:
    /// this object manages it.
    static ref NUTMEG_VIEW: nutmeg::View<MultiModel> =
        nutmeg::View::new(MultiModel::new(), nutmeg::Options::default()
            .destination(nutmeg::Destination::Stderr));
}

pub(super) fn add_bar(bar_id: usize) {
    NUTMEG_VIEW.update(|model| model.add_bar(bar_id));
}

/// Show progress on the global terminal progress bar,
/// or clear the bar if it's [Progress::None].
pub(super) fn update_bar(bar_id: usize, progress: Progress) {
    NUTMEG_VIEW.update(|model| model.update_bar(bar_id, progress));
}

pub(super) fn remove_bar(bar_id: usize) {
    let removed_last = NUTMEG_VIEW.update(|model| model.remove_bar(bar_id));
    if removed_last {
        NUTMEG_VIEW.clear();
    }
}

/// A stack of multiple Progress objects, each identified by an integer id.
///
/// Each entry corresponds to one progress::Bar in the abstract interface.
struct MultiModel(Vec<(usize, Progress)>);

impl MultiModel {
    const fn new() -> Self {
        MultiModel(Vec::new())
    }

    fn add_bar(&mut self, bar_id: usize) {
        assert!(
            !self.0.iter().any(|x| x.0 == bar_id),
            "task_id should not be already present"
        );
        self.0.push((bar_id, Progress::None));
    }

    fn update_bar(&mut self, bar_id: usize, progress: Progress) {
        let pos = self
            .0
            .iter()
            .position(|x| x.0 == bar_id)
            .expect("task_id should be present");
        self.0[pos].1 = progress;
    }

    fn remove_bar(&mut self, bar_id: usize) -> bool {
        self.0.retain(|(id, _)| *id != bar_id);
        self.0.is_empty()
    }
}

impl nutmeg::Model for MultiModel {
    fn render(&mut self, width: usize) -> String {
        self.0.iter_mut().map(|(_id, p)| p.render(width)).join("\n")
    }
}

impl nutmeg::Model for Progress {
    fn render(&mut self, _width: usize) -> String {
        match self {
            Progress::None => String::new(),
            Progress::Backup {
                filename,
                scanned_file_bytes,
                scanned_dirs,
                scanned_files,
                entries_new,
                entries_changed,
                entries_unchanged,
            } => format!(
                "\
                Scanned {dirs} directories, {files} files, {mb} MB\n\
                {new} new entries, {changed} changed, {unchanged} unchanged\n\
                {filename}",
                dirs = scanned_dirs.separate_with_commas(),
                files = scanned_files.separate_with_commas(),
                mb = (*scanned_file_bytes / 1_000_000).separate_with_commas(),
                new = entries_new.separate_with_commas(),
                changed = entries_changed.separate_with_commas(),
                unchanged = entries_unchanged.separate_with_commas(),
            ),
            Progress::DeleteBands {
                bands_done,
                total_bands,
            } => format!(
                "Delete bands: {}/{}...",
                bands_done.separate_with_commas(),
                total_bands.separate_with_commas(),
            ),
            Progress::DeleteBlocks {
                blocks_done,
                total_blocks,
            } => format!(
                "Delete blocks: {}/{}...",
                blocks_done.separate_with_commas(),
                total_blocks.separate_with_commas(),
            ),
            Progress::ListBlocks { count } => format!("List blocks: {count}..."),
            Progress::MeasureTree { files, total_bytes } => format!(
                "Measuring... {} files, {} MB",
                files.separate_with_commas(),
                (*total_bytes / 1_000_000).separate_with_commas()
            ),
            Progress::MeasureUnreferenced {
                blocks_done,
                blocks_total,
            } => format!(
                "Measure unreferenced blocks: {}/{}...",
                blocks_done.separate_with_commas(),
                blocks_total.separate_with_commas(),
            ),
            Progress::ReferencedBlocks {
                references_found,
                bands_started,
                total_bands,
                start,
            } => format!(
                "Find referenced blocks: {found} in {bands_started}/{total_bands} bands, {eta} remaining...",
                found = references_found.separate_with_commas(),
                eta = estimate_remaining(start, *bands_started, *total_bands),
            ),
            Progress::Restore {
                filename,
                bytes_done,
            } => format!(
                "Restoring: {mb} MB\n{filename}",
                mb = *bytes_done / 1_000_000,
            ),
            Progress::ValidateBlocks {
                blocks_done,
                total_blocks,
                bytes_done,
                start,
            } => {
                format!(
                    "Check block {}/{}: {} done, {} MB checked, {} remaining",
                    blocks_done.separate_with_commas(),
                    total_blocks.separate_with_commas(),
                    nutmeg::percent_done(*blocks_done, *total_blocks),
                    (*bytes_done / 1_000_000).separate_with_commas(),
                    nutmeg::estimate_remaining(start, *blocks_done, *total_blocks)
                )
            }
            Progress::ValidateBands {
                total_bands,
                bands_done,
                start,
            } => format!(
                "Check index {}/{}, {} done, {} remaining",
                bands_done,
                total_bands,
                nutmeg::percent_done(*bands_done, *total_bands),
                nutmeg::estimate_remaining(start, *bands_done, *total_bands)
            ),
        }
    }
}

pub(crate) struct WriteToNutmeg();

impl io::Write for WriteToNutmeg {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        NUTMEG_VIEW.message_bytes(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
