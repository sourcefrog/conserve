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

use std::fmt;

use crate::*;

#[derive(Default, Debug)]
pub struct DiffOptions {
    pub excludes: Option<GlobSet>,
    pub include_unchanged: bool,
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
    pub fn as_sigil(self) -> char {
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

impl fmt::Display for DiffEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\t{}", self.kind.as_sigil(), self.apath)
    }
}

/// Generate an iter of per-entry diffs between two trees.
pub fn diff(
    st: &StoredTree,
    lt: &LiveTree,
    options: &DiffOptions,
) -> Result<impl Iterator<Item = DiffEntry>> {
    let include_unchanged: bool = options.include_unchanged;
    Ok(MergeTrees::new(
        st.iter_filtered(None, options.excludes.clone())?,
        lt.iter_filtered(None, options.excludes.clone())?,
    )
    .map(move |me| diff_merged_entry(me))
    .filter(move |de: &DiffEntry| include_unchanged || de.kind != DiffKind::Unchanged))
}

fn diff_merged_entry<AE,BE>(me: merge::MergedEntry<AE,BE>) -> DiffEntry where AE:Entry,BE:Entry {
    use DiffKind::*;
    let kind = match me.kind {
        MergedEntryKind::Both(_,_) => Unchanged,
        MergedEntryKind::LeftOnly(_) => Deleted,
        MergedEntryKind::RightOnly(_) => New,
    };
    let de = DiffEntry {
        apath: me.apath,
        kind,
    };
    if kind == Deleted || kind == New {
        return de;
    }
    // TODO: Check metadata and file content before deciding that it's unchanged.
    de
}
