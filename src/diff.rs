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

use std::sync::Arc;

use crate::monitor::Monitor;
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

/// An async pseudo-iterator that yields a series of changes between two trees.
// TODO: This is barely any different to Merge, maybe we should just merge them?
// But, it does look a bit more at the contents of the entry, rather than just
// aligning by apath.
pub struct Diff {
    merge: MergeTrees,
    options: DiffOptions,
    // monitor: Arc<dyn Monitor>,
}

/// Generate an iter of per-entry diffs between two trees.
pub async fn diff(
    st: &StoredTree,
    lt: &SourceTree,
    options: DiffOptions,
    monitor: Arc<dyn Monitor>,
) -> Result<Diff> {
    let a = st.iter_entries(Apath::root(), options.exclude.clone(), monitor.clone());
    let b = lt.iter_entries(Apath::root(), options.exclude.clone(), monitor.clone())?;
    let merge = MergeTrees::new(a, b);
    Ok(Diff {
        merge,
        options,
        // monitor,
    })
}

impl Diff {
    pub async fn next(&mut self) -> Option<EntryChange> {
        while let Some(merge_entry) = self.merge.next().await {
            let ec = merge_entry.to_entry_change();
            if self.options.include_unchanged || !ec.change.is_unchanged() {
                return Some(ec);
            }
        }
        None
    }

    /// Collect all the diff entries.
    ///
    /// This is a convenience method for testing and small trees.
    pub async fn collect(&mut self) -> Vec<EntryChange> {
        let mut changes = Vec::new();
        while let Some(change) = self.next().await {
            changes.push(change);
        }
        changes
    }
}
