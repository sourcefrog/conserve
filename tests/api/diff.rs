// Copyright 2021-2024 Martin Pool.

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

use filetime::{set_file_mtime, FileTime};
use itertools::Itertools;

use conserve::monitor::collect::CollectMonitor;
use conserve::test_fixtures::{ScratchArchive, TreeFixture};
use conserve::*;

/// Make a tree with one file and an archive with one version.
fn create_tree() -> (ScratchArchive, TreeFixture) {
    let a = ScratchArchive::new();
    let tf = TreeFixture::new();
    tf.create_file_with_contents("thing", b"contents of thing");
    let stats = backup(
        &a,
        tf.path(),
        &BackupOptions::default(),
        CollectMonitor::arc(),
    )
    .unwrap();
    assert_eq!(stats.new_files, 1);
    (a, tf)
}

#[test]
fn diff_unchanged() {
    let (a, tf) = create_tree();

    let st = a.open_stored_tree(BandSelectionPolicy::Latest).unwrap();

    let options = DiffOptions {
        include_unchanged: true,
        ..DiffOptions::default()
    };
    let changes: Vec<EntryChange> = diff(&st, &tf.live_tree(), &options).unwrap().collect();
    dbg!(&changes);
    assert_eq!(changes.len(), 2); // Root directory and the file "/thing".
    assert_eq!(changes[0].apath, "/");
    assert!(changes[0].change.is_unchanged());
    assert!(!changes[0].change.is_changed());
    assert_eq!(changes[1].apath, "/thing");
    assert!(changes[1].change.is_unchanged());
    assert!(!changes[1].change.is_changed());

    // Excluding unchanged elements
    let options = DiffOptions {
        include_unchanged: false,
        ..DiffOptions::default()
    };
    let changes = diff(&st, &tf.live_tree(), &options).unwrap().collect_vec();
    println!("changes with include_unchanged=false:\n{changes:#?}");
    assert_eq!(changes.len(), 0);
}

#[test]
fn mtime_only_change_reported_as_changed() {
    let (a, tf) = create_tree();

    let st = a.open_stored_tree(BandSelectionPolicy::Latest).unwrap();
    set_file_mtime(
        tf.path().join("thing"),
        FileTime::from_unix_time(1704135090, 0),
    )
    .unwrap();

    let options = DiffOptions {
        include_unchanged: false,
        ..DiffOptions::default()
    };
    let changes: Vec<EntryChange> = diff(&st, &tf.live_tree(), &options).unwrap().collect();
    dbg!(&changes);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].apath, "/thing");
    assert!(changes[0].change.is_changed());
    assert!(!changes[0].change.is_unchanged());
}

// Test only on Linux, as macOS doesn't seem to have a way to get all groups
// (see https://docs.rs/nix/latest/nix/unistd/fn.getgroups.html).
#[cfg(target_os = "linux")]
#[test]
fn chgrp_reported_as_changed() {
    use std::os::unix::fs::chown;

    use conserve::test_fixtures::arbitrary_secondary_group;

    let (a, tf) = create_tree();

    chown(
        tf.path().join("thing"),
        None,
        Some(arbitrary_secondary_group()),
    )
    .unwrap();
    let st = a.open_stored_tree(BandSelectionPolicy::Latest).unwrap();

    let options = DiffOptions {
        include_unchanged: false,
        ..DiffOptions::default()
    };
    let changes: Vec<EntryChange> = diff(&st, &tf.live_tree(), &options).unwrap().collect();
    dbg!(&changes);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].apath, "/thing");
    assert!(changes[0].change.is_changed());
    assert!(!changes[0].change.is_unchanged());
}

#[cfg(unix)]
#[test]
fn symlink_target_change_reported_as_changed() {
    use std::fs::remove_file;
    use std::path::Path;

    let a = ScratchArchive::new();
    let tf = TreeFixture::new();
    tf.create_symlink("link", "target");
    backup(
        &a,
        tf.path(),
        &BackupOptions::default(),
        CollectMonitor::arc(),
    )
    .unwrap();

    let link_path = tf.path().join("link");
    remove_file(&link_path).unwrap();
    std::os::unix::fs::symlink("new-target", &link_path).unwrap();
    let st = a.open_stored_tree(BandSelectionPolicy::Latest).unwrap();
    assert_eq!(
        std::fs::read_link(&link_path).unwrap(),
        Path::new("new-target")
    );

    let options = DiffOptions {
        include_unchanged: false,
        ..DiffOptions::default()
    };
    let changes: Vec<EntryChange> = diff(&st, &tf.live_tree(), &options).unwrap().collect();
    dbg!(&changes);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].apath, "/link");
    assert!(changes[0].change.is_changed());
    assert!(!changes[0].change.is_unchanged());
}
