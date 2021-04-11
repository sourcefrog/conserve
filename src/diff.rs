// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2021 Martin Pool.

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

use crate::*;

#[derive(Default, Debug)]
pub struct DiffOptions {
    pub excludes: Option<GlobSet>,
}

/// The overall state of change of an entry.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DiffKind {
    Unchanged,
    New,
    Deleted,
    Changed,
}

impl DiffKind {
    pub fn as_char(self) -> char {
        use DiffKind::*;
        match self {
            Unchanged => '.',
            New => '+',
            Deleted => '-',
            Changed => '*',
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DiffEntry {
    pub apath: Apath,
    pub kind: DiffKind,
}

/// Generate an iter of per-entry diffs between two trees.
pub fn diff(
    st: &StoredTree,
    lt: &LiveTree,
    options: &DiffOptions,
) -> Result<impl Iterator<Item = DiffEntry>> {
    Ok(MergeTrees::new(
        st.iter_filtered(None, options.excludes.clone())?,
        lt.iter_filtered(None, options.excludes.clone())?,
    )
    .map(|me| {
        let kind = match me.kind {
            MergedEntryKind::Both => DiffKind::Unchanged,
            MergedEntryKind::LeftOnly => DiffKind::Deleted,
            MergedEntryKind::RightOnly => DiffKind::New,
        };
        // TODO: Check metadata and file content before deciding that it's unchanged.
        DiffEntry {
            apath: me.apath,
            kind,
        }
    }))
}
