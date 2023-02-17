// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2021, 2022 Martin Pool.

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

use readahead_iterator::IntoReadahead;
use serde::Serialize;

use crate::*;

use DiffKind::*;
use Kind::*;
use MergedEntryKind::*;

#[derive(Debug)]
pub struct DiffOptions {
    pub exclude: Exclude,
    pub include_unchanged: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        DiffOptions {
            exclude: Exclude::nothing(),
            include_unchanged: false,
        }
    }
}

/// The overall state of change of an entry.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub enum DiffKind {
    Unchanged,
    New,
    Deleted,
    Changed,
}

impl DiffKind {
    pub fn as_sigil(self) -> char {
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
    let readahead = 1000;
    let include_unchanged: bool = options.include_unchanged;
    // TODO: Take an option for the subtree?
    let ait = st
        .iter_entries(Apath::root(), options.exclude.clone())?
        .readahead(readahead);
    let bit = lt
        .iter_entries(Apath::root(), options.exclude.clone())?
        .filter(|le| le.kind() != Unknown)
        .readahead(readahead);
    Ok(MergeTrees::new(ait, bit)
        .map(diff_merged_entry)
        .filter(move |de: &DiffEntry| include_unchanged || de.kind != DiffKind::Unchanged))
}

fn diff_merged_entry<AE, BE>(me: merge::MergedEntry<AE, BE>) -> DiffEntry
where
    AE: Entry,
    BE: Entry,
{
    let apath = me.apath;
    match me.kind {
        Both(ae, be) => diff_common_entry(ae, be, apath),
        LeftOnly(_) => DiffEntry {
            kind: Deleted,
            apath,
        },
        RightOnly(_) => DiffEntry { kind: New, apath },
    }
}

fn diff_common_entry<AE, BE>(ae: AE, be: BE, apath: Apath) -> DiffEntry
where
    AE: Entry,
    BE: Entry,
{
    // TODO: Actually compare content, if requested.
    // TODO: Skip Kind::Unknown.
    let ak = ae.kind();
    if ak != be.kind()
        || (ak == File && (ae.mtime() != be.mtime() || ae.size() != be.size()))
        || (ak == Symlink && (ae.symlink_target() != be.symlink_target()))
    {
        DiffEntry {
            kind: Changed,
            apath,
        }
    } else {
        DiffEntry {
            kind: Unchanged,
            apath,
        }
    }
}
