// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use std::io::prelude::*;
use std::io::{stdout, BufWriter};

use crate::*;

pub struct DiffOptions {
    pub excludes: Option<GlobSet>,
}

pub fn diff(st: &StoredTree, lt: &LiveTree, options: &DiffOptions) -> Result<()> {
    // TODO: Consider whether the actual files have changed.
    // TODO: Summarize diff.
    // TODO: Optionally include unchanged files.

    show_tree_diff(
        &mut MergeTrees::new(
            st.iter_filtered(None, options.excludes.clone())?,
            lt.iter_filtered(None, options.excludes.clone())?,
        ),
        &mut stdout(),
    )
}

fn show_tree_diff(
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
