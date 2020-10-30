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
fn small_files_combined_two_backups() {
    let mut af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file("file1");
    srcdir.create_file("file2");

    let stats1 = backup(&mut af, &srcdir.live_tree(), &BackupOptions::default()).unwrap();
    // Although the two files have the same content, we do not yet dedupe them
    // within a combined block, so the block is different to when one identical
    // file is stored alone. This could be fixed.
    assert_eq!(stats1.combined_blocks, 1);
    assert_eq!(stats1.new_files, 2);
    assert_eq!(stats1.written_blocks, 1);
    assert_eq!(stats1.new_files, 2);

    // Add one more file, also identical, but it is not combined with the previous blocks.
    // This is a shortcoming of the current dedupe approach.
    srcdir.create_file("file3");
    let stats2 = backup(&mut af, &srcdir.live_tree(), &BackupOptions::default()).unwrap();
    assert_eq!(stats2.new_files, 1);
    assert_eq!(stats2.unmodified_files, 2);
    assert_eq!(stats2.written_blocks, 1);
    assert_eq!(stats2.combined_blocks, 1);

    assert_eq!(af.block_dir().block_names().unwrap().count(), 2);
}

#[test]
fn many_small_files_combined_to_one_block() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    // The directory also counts as an entry, so we should be able to fit 1999
    // files in 2 hunks of 1000 entries.
    for i in 0..1999 {
        srcdir.create_file_of_length_with_prefix(
            &format!("file{:04}", i),
            200,
            format!("something about {}", i).as_bytes(),
        );
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
    assert_eq!(stats.small_combined_files, 1999);
    assert_eq!(stats.errors, 0);
    // We write two combined blocks
    assert_eq!(stats.written_blocks, 2);
    assert_eq!(stats.combined_blocks, 2);

    let tree = af.open_stored_tree(BandSelectionPolicy::Latest).unwrap();
    let mut entry_iter = tree.iter_entries().unwrap();
    assert_eq!(entry_iter.next().unwrap().apath(), "/");
    for (i, entry) in entry_iter.enumerate() {
        assert_eq!(entry.apath().to_string(), format!("/file{:04}", i));
    }
    assert_eq!(tree.iter_entries().unwrap().count(), 2000);
}

#[test]
pub fn mixed_medium_small_files_two_hunks() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    const MEDIUM_LENGTH: u64 = 150_000;
    // Make some files large enough not to be grouped together as small files.
    for i in 0..1999 {
        let name = format!("file{:04}", i);
        if i % 100 == 0 {
            srcdir.create_file_of_length_with_prefix(&name, MEDIUM_LENGTH, b"something");
        } else {
            srcdir.create_file(&name);
        }
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
    assert_eq!(stats.single_block_files, 20);
    assert_eq!(stats.small_combined_files, 1999 - 20);
    assert_eq!(stats.errors, 0);
    // There's one deduped block for all the large files, and then one per hunk for all the small combined files.
    assert_eq!(stats.written_blocks, 3);

    let tree = af.open_stored_tree(BandSelectionPolicy::Latest).unwrap();
    let mut entry_iter = tree.iter_entries().unwrap();
    assert_eq!(entry_iter.next().unwrap().apath(), "/");
    for (i, entry) in entry_iter.enumerate() {
        assert_eq!(entry.apath().to_string(), format!("/file{:04}", i));
    }
    assert_eq!(tree.iter_entries().unwrap().count(), 2000);
}
