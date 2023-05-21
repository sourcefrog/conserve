// Conserve backup system.
// Copyright 2020-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use assert_fs::TempDir;

use assert_fs::prelude::*;
use conserve::backup;
use conserve::Archive;

#[test]
fn truncated_band_head() {
    let mut archive_dir = TempDir::new().unwrap();
    let mut source_dir = TempDir::new().unwrap();

    let mut archive = Archive::create_path(archive_dir.path()).expect("create archive");
    source_dir
        .child("file")
        .write_str("content in first backup")
        .unwrap();

    // backup(&mut archive, source_dir.path());
}
