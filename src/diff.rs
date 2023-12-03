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

//! Diff two trees: for example a live tree against a stored tree.
//!
//! See also [conserve::show_diff] to format the diff as text.

use readahead_iterator::IntoReadahead;

use crate::*;

#[derive(Debug)]
pub struct DiffOptions {
    pub exclude: Exclude,
    pub include_unchanged: bool,
    // TODO: An option to filter to a subtree?
    // TODO: Optionally compare all the content?
}

impl Default for DiffOptions {
    fn default() -> Self {
        DiffOptions {
            exclude: Exclude::nothing(),
            include_unchanged: false,
        }
    }
}

/// Generate an iter of per-entry diffs between two trees.
pub fn diff(
    st: &StoredTree,
    lt: &LiveTree,
    options: &DiffOptions,
) -> Result<impl Iterator<Item = EntryChange>> {
    let readahead = 1000;
    let include_unchanged: bool = options.include_unchanged; // Copy out to avoid lifetime problems in the callback
    let ait = st
        .iter_entries(Apath::root(), options.exclude.clone())?
        .readahead(readahead);
    let bit = lt
        .iter_entries(Apath::root(), options.exclude.clone())?
        .filter(|le| le.kind() != Kind::Unknown)
        .readahead(readahead);
    Ok(MergeTrees::new(ait, bit)
        .map(|me| me.to_entry_change())
        .filter(move |c: &EntryChange| include_unchanged || !c.is_unchanged()))
}
