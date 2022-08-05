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

use conserve::backup::BackupMonitor;
use conserve::ui::duration_to_hms;
use conserve::{Archive, Result, Band, BandSelectionPolicy, Exclude, bytes_to_human_mb, IndexEntry, DiffEntry, ReadTree, Kind, Entry, DiffKind};
use tracing::{warn, info};
use nutmeg::View;

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
pub fn show_versions(
    archive: &Archive,
    options: &ShowVersionsOptions,
) -> Result<()> {
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
            let tree_mb_str = bytes_to_human_mb(
                archive
                    .open_stored_tree(BandSelectionPolicy::Specified(band_id.clone()))?
                    .size(Exclude::nothing())?
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

pub struct NutmegBackupMonitor<'a> {
    view: &'a View<BackupProgressModel>,
}

impl<'a> NutmegBackupMonitor<'a> {
    pub fn new(view: &'a View<BackupProgressModel>) -> Self {
        Self { view }
    }
}

impl BackupMonitor for NutmegBackupMonitor<'_> {
    fn copy(&mut self, entry: &conserve::LiveEntry) {
        self.view.update(|model| {
            model.filename = entry.apath().to_string();
            match entry.kind() {
                Kind::Dir => model.scanned_dirs += 1,
                Kind::File => model.scanned_files += 1,
                _ => (),
            }
        });
    }

    fn copy_result(&mut self, entry: &conserve::LiveEntry, result: &Option<conserve::DiffKind>) {
        if let Some(diff_kind) = result.as_ref() {
            self.view.update(|model| match diff_kind {
                &DiffKind::Changed => model.entries_changed += 1,
                &DiffKind::New => model.entries_new += 1,
                &DiffKind::Unchanged => model.entries_unchanged += 1,
                &DiffKind::Deleted => model.entries_deleted += 1,
            })
        }

        if let Some(size) = entry.size() {
            self.view.update(|model| model.scanned_file_bytes += size);
        }
    }

    fn copy_error(&mut self, entry: &conserve::LiveEntry, _error: &conserve::Error) {
        if let Some(size) = entry.size() {
            self.view.update(|model| model.scanned_file_bytes += size);
        }
    }
}