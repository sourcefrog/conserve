// Copyright 2015, 2016, 2017, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Tests focussed on backup behavior.

use conserve::test_fixtures::ScratchArchive;
use conserve::test_fixtures::TreeFixture;
use conserve::*;

#[test]
pub fn many_files_multiple_hunks() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    // The directory also counts as an entry, so we should be able to fit 1999
    // files in 2 hunks of 1000 entries.
    for i in 0..1999 {
        srcdir.create_file(&format!("file{:04}", i));
    }
    let stats = backup(&af, &srcdir.live_tree(), &BackupOptions::default()).expect("backup");
    assert_eq!(
        stats.index_builder_stats.index_hunks, 2,
        "expect exactly 2 hunks"
    );
    assert_eq!(stats.files, 1999);
    assert_eq!(stats.directories, 1);
    assert_eq!(stats.unknown_kind, 0);

    assert_eq!(stats.new_files, 1999);
    assert_eq!(stats.single_block_files, 1999);
    assert_eq!(stats.errors, 0);
    // They all have the same content.
    assert_eq!(stats.written_blocks, 1);

    let tree = af.open_stored_tree(BandSelectionPolicy::Latest).unwrap();
    let mut entry_iter = tree.iter_entries().unwrap();
    assert_eq!(entry_iter.next().unwrap().apath(), "/");
    for (i, entry) in entry_iter.enumerate() {
        assert_eq!(entry.apath().to_string(), format!("/file{:04}", i));
    }
}
