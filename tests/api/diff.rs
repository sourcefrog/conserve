// Copyright 2021 Martin Pool.

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

#[test]
fn diff_unchanged() {
    let a = ScratchArchive::new();
    let tf = TreeFixture::new();

    tf.create_file_with_contents("thing", b"contents of thing");
    let lt = tf.live_tree();
    let stats = backup(&a, &lt, &BackupOptions::default(), None).unwrap();
    assert_eq!(stats.new_files, 1);

    let st = a.open_stored_tree(BandSelectionPolicy::Latest).unwrap();

    let options = DiffOptions {
        include_unchanged: true,
        ..DiffOptions::default()
    };
    let des: Vec<DiffEntry> = diff(&st, &lt, &options).unwrap().collect();
    assert_eq!(des.len(), 2); // Root directory and the file "/thing".
    assert_eq!(
        des[0],
        DiffEntry {
            apath: "/".into(),
            kind: DiffKind::Unchanged,
        }
    );
    assert_eq!(
        des[1],
        DiffEntry {
            apath: "/thing".into(),
            kind: DiffKind::Unchanged,
        }
    );

    // Excluding unchanged elements
    let options = DiffOptions {
        include_unchanged: false,
        ..DiffOptions::default()
    };

    assert_eq!(diff(&st, &lt, &options).unwrap().count(), 0);
}
