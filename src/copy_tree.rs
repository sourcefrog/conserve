// Conserve backup system.
// Copyright 2017, 2018, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Copy tree contents.

use crate::kind::Kind;
use crate::stats::CopyStats;
use crate::*;

#[derive(Default, Clone, Debug)]
pub struct CopyOptions {
    pub print_filenames: bool,
    pub measure_first: bool,
    /// Copy only this subtree from the source.
    pub only_subtree: Option<Apath>,
    pub excludes: Option<GlobSet>,
}

/// Copy files and other entries from one tree to another.
///
/// NOTE: Although this is public, it's suggested to use `Archive::backup` or `Archive::restore` if
/// possible, as they're higher-level APIs.
pub fn copy_tree<ST: ReadTree, DT: WriteTree>(
    source: &ST,
    mut dest: DT,
    options: &CopyOptions,
) -> Result<CopyStats> {
    let mut stats = CopyStats::default();
    let mut progress_bar = ProgressBar::new();
    // This causes us to walk the source tree twice, which is probably an acceptable option
    // since it's nice to see realistic overall progress. We could keep all the entries
    // in memory, and maybe we should, but it might get unreasonably big.
    if options.measure_first {
        progress_bar.set_phase("Measure source tree");
        // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
        // again a second time? But, that'll potentially use memory proportional to tree size, which
        // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
        // deleted or changed while this is running.
        progress_bar.set_bytes_total(source.size(options.excludes.clone())?.file_bytes as u64);
    }

    progress_bar.set_phase("Copying");
    let entry_iter: Box<dyn Iterator<Item = ST::Entry>> =
        source.iter_filtered(options.only_subtree.clone(), options.excludes.clone())?;
    for entry in entry_iter {
        if options.print_filenames {
            crate::ui::println(entry.apath());
        }
        progress_bar.set_filename(entry.apath().to_string());
        if let Err(e) = match entry.kind() {
            Kind::Dir => {
                stats.directories += 1;
                dest.copy_dir(&entry)
            }
            Kind::File => {
                stats.files += 1;
                let result = dest.copy_file(&entry, source).map(|s| stats += s);
                if let Some(bytes) = entry.size() {
                    progress_bar.increment_bytes_done(bytes);
                }
                result
            }
            Kind::Symlink => {
                stats.symlinks += 1;
                dest.copy_symlink(&entry)
            }
            Kind::Unknown => {
                stats.unknown_kind += 1;
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                continue;
            }
        } {
            ui::show_error(&e);
            stats.errors += 1;
            continue;
        }
    }
    stats += dest.finish()?;
    // TODO: Merge in stats from the tree iter and maybe the source tree?
    Ok(stats)
}
