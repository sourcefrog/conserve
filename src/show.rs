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
use std::io::{BufWriter, Write};

use crate::*;

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

/// Print a list of versions, one per line.
pub fn show_versions(
    archive: &Archive,
    options: &ShowVersionsOptions,
    w: &mut dyn Write,
) -> Result<()> {
    let mut band_ids = archive.list_band_ids()?;
    if options.newest_first {
        band_ids.reverse();
    }
    for band_id in band_ids {
        if !(options.tree_size || options.start_time || options.backup_duration) {
            writeln!(w, "{}", band_id)?;
            continue;
        }
        let mut l: Vec<String> = Vec::new();
        l.push(format!("{:<20}", band_id));
        let band = match Band::open(&archive, &band_id) {
            Ok(band) => band,
            Err(e) => {
                ui::problem(&format!("Failed to open band {:?}: {:?}", band_id, e));
                continue;
            }
        };
        let info = match band.get_info() {
            Ok(info) => info,
            Err(e) => {
                ui::problem(&format!("Failed to read band tail {:?}: {:?}", band_id, e));
                continue;
            }
        };

        if options.start_time {
            let start_time = info.start_time;
            let start_time_str = if options.utc {
                start_time.format(crate::TIMESTAMP_FORMAT)
            } else {
                start_time
                    .with_timezone(&chrono::Local)
                    .format(crate::TIMESTAMP_FORMAT)
            };
            l.push(format!("{:<10}", start_time_str));
        }

        if options.backup_duration {
            let duration_str: Cow<str> = if info.is_closed {
                info.end_time
                    .and_then(|et| (et - info.start_time).to_std().ok())
                    .map(crate::ui::duration_to_hms)
                    .map(|s| Cow::Owned(s))
                    .unwrap_or(Cow::Borrowed("unknown"))
            } else {
                Cow::Borrowed("incomplete")
            };
            l.push(format!("{:>10}", duration_str));
        }

        if options.tree_size {
            let tree_mb_str = crate::misc::bytes_to_human_mb(
                archive
                    .open_stored_tree(BandSelectionPolicy::Specified(band_id.clone()))?
                    .size(None)?
                    .file_bytes,
            );
            l.push(format!("{:>14}", tree_mb_str,));
        }

        writeln!(w, "{}", l.join(" "))?;
    }
    Ok(())
}

pub fn show_index_json(band: &Band, w: &mut dyn Write) -> Result<()> {
    // TODO: Maybe use https://docs.serde.rs/serde/ser/trait.Serializer.html#method.collect_seq.
    let bw = BufWriter::new(w);
    let index_entries: Vec<IndexEntry> = band.iter_entries().collect();
    serde_json::ser::to_writer_pretty(bw, &index_entries)
        .map_err(|source| Error::SerializeIndex { source })
}

pub fn show_entry_names<E: Entry, I: Iterator<Item = E>>(it: I, w: &mut dyn Write) -> Result<()> {
    let mut bw = BufWriter::new(w);
    for entry in it {
        writeln!(bw, "{}", entry.apath())?;
    }
    Ok(())
}
