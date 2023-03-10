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

//! Tests for the diff API.

use conserve::test_fixtures::{ScratchArchive, TreeFixture};
use conserve::*;
use itertools::Itertools;

#[test]
fn diff_unchanged() {
    let a = ScratchArchive::new();
    let tf = TreeFixture::new();

    tf.create_file_with_contents("thing", b"contents of thing");
    let lt = tf.live_tree();
    let stats = backup(&a, &lt, &BackupOptions::default()).unwrap();
    assert_eq!(stats.new_files, 1);

    let st = a.open_stored_tree(BandSelectionPolicy::Latest).unwrap();

    let options = DiffOptions {
        include_unchanged: true,
        ..DiffOptions::default()
    };
    let changes: Vec<EntryChange> = diff(&st, &lt, &options).unwrap().collect();
    dbg!(&changes);
    assert_eq!(changes.len(), 2); // Root directory and the file "/thing".
    assert_eq!(changes[0].apath, "/");
    assert!(changes[0].is_unchanged());
    assert_eq!(changes[1].apath, "/thing");
    assert!(changes[1].is_unchanged());

    // Excluding unchanged elements
    let options = DiffOptions {
        include_unchanged: false,
        ..DiffOptions::default()
    };
    let changes = diff(&st, &lt, &options).unwrap().collect_vec();
    println!("changes with include_unchanged=false:\n{changes:#?}");
    assert_eq!(changes.len(), 0);
}
