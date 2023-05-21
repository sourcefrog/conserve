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

use std::fs::read_to_string;

use assert_fs::prelude::*;
use assert_fs::TempDir;
use predicates::prelude::*;

use conserve::backup;
use conserve::Archive;
use conserve::BackupOptions;
use tracing_test::traced_test;

// TODO: Also test other files.
// TODO: Also test other types of damage, including missing files,
// permission denied (as a kind of IOError), and binary junk.

#[traced_test]
#[test]
#[should_panic(expected = "Failed to open band: DeserializeJson")] // TODO: Should pass!
fn truncated_band_head() {
    let archive_dir = TempDir::new().unwrap();
    let source_dir = TempDir::new().unwrap();

    let mut archive = Archive::create_path(archive_dir.path()).expect("create archive");
    source_dir
        .child("file")
        .write_str("content in first backup")
        .unwrap();

    let backup_options = BackupOptions::default();
    backup(&mut archive, source_dir.path(), &backup_options).expect("initial backup");

    let bandhead = archive_dir.child("b0000").child("BANDHEAD");
    bandhead.assert(predicate::path::exists());
    println!(
        "initial bandhead contents: {:?}",
        read_to_string(&bandhead).expect("read bandhead")
    );
    bandhead.write_str("").expect("truncate bandhead");

    // A second backup should succeed.
    source_dir
        .child("file")
        .write_str("content in second backup")
        .unwrap();

    backup(&mut archive, source_dir.path(), &backup_options)
        .expect("write second backup even though first bandhead is damaged");
}
