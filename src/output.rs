// Conserve backup system.
// Copyright 2018, 2020 Martin Pool.

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

use std::io::{BufWriter, Write};

use chrono::Local;

use crate::*;

pub fn show_brief_version_list(archive: &Archive, w: &mut dyn Write) -> Result<()> {
    for band_id in archive.list_band_ids()? {
        writeln!(w, "{}", band_id)?
    }
    Ok(())
}

pub fn show_verbose_version_list(
    archive: &Archive,
    show_sizes: bool,
    w: &mut dyn Write,
) -> Result<()> {
    for band_id in archive.list_band_ids()? {
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
        let is_complete_str = if info.is_closed {
            "complete"
        } else {
            "incomplete"
        };
        let start_time_str = info
            .start_time
            .with_timezone(&Local)
            .format(crate::TIMESTAMP_FORMAT);
        let duration_str = info
            .end_time
            .and_then(|et| (et - info.start_time).to_std().ok())
            .map(crate::ui::duration_to_hms)
            .unwrap_or_default();
        if show_sizes {
            let tree_mb = crate::misc::bytes_to_human_mb(
                StoredTree::open_incomplete_version(archive, &band.id())?
                    .size()?
                    .file_bytes,
            );
            writeln!(
                w,
                "{:<20} {:<10} {} {:>8} {:>14}",
                band_id, is_complete_str, start_time_str, duration_str, tree_mb,
            )?;
        } else {
            writeln!(
                w,
                "{:<20} {:<10} {} {:>8}",
                band_id, is_complete_str, start_time_str, duration_str,
            )?;
        }
    }
    Ok(())
}

pub fn show_index_json(band: &Band, w: &mut dyn Write) -> Result<()> {
    // TODO: Maybe use https://docs.serde.rs/serde/ser/trait.Serializer.html#method.collect_seq.
    let bw = BufWriter::new(w);
    let index_entries: Vec<IndexEntry> = band.iter_entries()?.collect();
    serde_json::ser::to_writer_pretty(bw, &index_entries)
        .map_err(|source| Error::SerializeIndex { source })
}

pub fn show_tree_names<T: ReadTree>(tree: &T, w: &mut dyn Write) -> Result<()> {
    let mut bw = BufWriter::new(w);
    for entry in tree.iter_entries()? {
        writeln!(bw, "{}", entry.apath())?;
    }
    Ok(())
}

pub fn show_tree_diff(
    iter: &mut dyn Iterator<Item = crate::merge::MergedEntry>,
    w: &mut dyn Write,
) -> Result<()> {
    let mut bw = BufWriter::new(w);
    for e in iter {
        let ks = match e.kind {
            MergedEntryKind::LeftOnly => "left",
            MergedEntryKind::RightOnly => "right",
            MergedEntryKind::Both => "both",
        };
        writeln!(bw, "{:<8} {}", ks, e.apath)?;
    }
    Ok(())
}
