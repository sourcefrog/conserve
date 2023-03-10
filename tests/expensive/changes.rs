// Conserve backup system.
// Copyright 2022-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Try backing up and restoring various sequences of changes to a tree.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use proptest::prelude::*;
use proptest_derive::Arbitrary;
use tempfile::TempDir;

use conserve::test_fixtures::*;
use conserve::*;

/// A change to a single file in a tree.
///
/// The arbitrary operations are constructed so that we'll tend to
/// revisit already-used files, rather than making many files
/// that are scattered through the namespace and only touched once.
/// At the same time we do need the operations to be absolutely
/// deterministic from the seed input.
///
/// Files are numbered sequentially in the order they're added.
#[derive(Debug, Clone, Arbitrary)]
enum TreeChange {
    /// Add a file.
    AddFile,
    /// Make a backup.
    Backup,
    /// Select file `.0` modulo the set of files remaining in the
    /// tree, and delete it. Do nothing if the tree is empty.
    RemoveFile(usize),
    /// Select file `.0` modulo the set of files remaining in the
    /// tree, and change its contents. Do nothing if the tree is empty.
    ChangeFile(usize),
    /// Restore version `i%n`.
    Restore(usize),
}

fn backup_sequential_changes(changes: &[TreeChange]) {
    use TreeChange::*;
    let tf = TreeFixture::new();
    let archive = ScratchArchive::new();
    let mut live_files: Vec<String> = Vec::new();
    let mut live_contents: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut next_file = 0;
    // Trees containing a naive copy of the source at each backup.
    let mut backup_contents: Vec<TempDir> = Vec::new();
    for (i, c) in changes.iter().enumerate() {
        println!("{i}: {c:?}");
        match c {
            AddFile => {
                let content = format!("initial content of {next_file}").into_bytes();
                let name = next_file.to_string();
                tf.create_file_with_contents(&name, &content);
                live_files.push(name.clone());
                live_contents.insert(name, content);
                next_file += 1;
            }
            ChangeFile(j) => {
                if !live_files.is_empty() {
                    let j = j % live_files.len();
                    let name = &live_files[j];
                    let content = format!("changed content of {j} from step {i}").into_bytes();
                    fs::write(tf.path().join(name), content).unwrap();
                }
            }
            RemoveFile(j) => {
                if !live_files.is_empty() {
                    let j = j % live_files.len();
                    let name = live_files.remove(j);
                    live_contents.remove(&name);
                    fs::remove_file(tf.path().join(&name)).unwrap();
                }
            }
            Backup => {
                // Wait a little bit to let files get distinct mtimes: very
                // close-spaced updates might not be distinguishable by mtime.
                std::thread::sleep(std::time::Duration::from_millis(10));
                let options = BackupOptions {
                    max_entries_per_hunk: 3,
                    ..BackupOptions::default()
                };
                backup(&archive, &tf.live_tree(), &options).unwrap();
                let snapshot = TempDir::new().unwrap();
                cp_r::CopyOptions::default()
                    .copy_tree(tf.path(), snapshot.path())
                    .unwrap();
                backup_contents.push(snapshot);
            }
            Restore(i_version) => {
                if !backup_contents.is_empty() {
                    let version = i_version % backup_contents.len();
                    check_restore_against_snapshot(
                        &archive,
                        BandId::new(&[version as u32]),
                        backup_contents[version].path(),
                    );
                }
            }
        }
    }
    for (i, snapshot) in backup_contents.iter().enumerate() {
        check_restore_against_snapshot(&archive, BandId::new(&[i as u32]), snapshot.path())
    }
    println!(">> done!");
}

fn check_restore_against_snapshot(archive: &Archive, band_id: BandId, snapshot: &Path) {
    let restore_dir = tempfile::tempdir().unwrap();
    // TODO: Select the right band.
    let options = RestoreOptions {
        band_selection: BandSelectionPolicy::Specified(band_id),
        ..RestoreOptions::default()
    };
    restore(archive, restore_dir.path(), &options).unwrap();
    dir_assert::assert_paths(restore_dir.path(), snapshot).unwrap();
}

proptest! {
    // The bulk of the tests are kept outside this macro so that they
    // are more understandable to rust-analyzer.

    #[test]
    #[ignore] // making all these backups is expensive
    fn changes(changes: Vec<TreeChange>) {
        backup_sequential_changes(&changes);
    }
}
