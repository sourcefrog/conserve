// Copyright 2021-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Tests for the Conserve library API.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

mod apath;
mod archive;
mod backup;
mod bandid;
mod blockhash;
mod damaged;
mod delete;
mod diff;
mod format_flags;
mod gc;
mod live_tree;
mod old_archives;
mod restore;
mod transport;

/// Make a copy of a archive testdata.
fn copy_testdata_archive(name: &str, version: &str) -> TempDir {
    let temp = TempDir::with_prefix(format!("conserve-api-test-{}-{}", name, version))
        .expect("create temp dir");
    let stored_archive_path = testdata_archive_path(name, version);
    cp_r::CopyOptions::default()
        .copy_tree(stored_archive_path, temp.path())
        .expect("copy archive tree");
    temp
}

fn testdata_archive_path(name: &str, version: &str) -> PathBuf {
    Path::new("testdata/archive")
        .join(name)
        .join(format!("v{version}/"))
}
