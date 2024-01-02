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

use std::fs::rename;
use std::sync::Arc;

use assert_fs::prelude::*;
use assert_fs::TempDir;
use dir_assert::assert_paths;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use rstest::rstest;
use tracing_test::traced_test;
// use predicates::prelude::*;

use conserve::monitor::collect::CollectMonitor;
use conserve::{
    backup, restore, Apath, Archive, BackupOptions, BandId, BandSelectionPolicy, EntryTrait,
    Exclude, RestoreOptions, ValidateOptions,
};

mod damage;
use damage::{DamageAction, DamageLocation};

// TODO: Test restore from a partially damaged backup.
// TODO: Test that you can delete a damaged backup; then there are no problems.

/// Changes that can be made to a tree and then backed up.
#[derive(Debug, Clone)]
enum TreeChanges {
    None,
    AlterFile,
    RenameFile,
}

impl TreeChanges {
    fn apply(&self, dir: &TempDir) {
        match self {
            TreeChanges::None => {}
            TreeChanges::AlterFile => {
                dir.child("file").write_str("changed").unwrap();
            }
            TreeChanges::RenameFile => {
                rename(dir.child("file"), dir.child("file2")).unwrap();
            }
        }
    }
}

#[rstest]
#[traced_test]
#[test]
fn backup_after_damage(
    #[values(DamageAction::Delete, DamageAction::Truncate)] action: DamageAction,
    #[values(
        DamageLocation::BandHead(0),
        DamageLocation::BandTail(0),
        DamageLocation::Block(0)
    )]
    location: DamageLocation,
    #[values(TreeChanges::None, TreeChanges::AlterFile, TreeChanges::RenameFile)]
    changes: TreeChanges,
) {
    let archive_dir = TempDir::new().unwrap();
    let source_dir = TempDir::new().unwrap();

    let archive = Archive::create_path(archive_dir.path()).expect("create archive");
    source_dir
        .child("file")
        .write_str("content in first backup")
        .unwrap();

    let backup_options = BackupOptions::default();
    backup(
        &archive,
        source_dir.path(),
        &backup_options,
        CollectMonitor::arc(),
    )
    .expect("initial backup");

    drop(archive);
    action.damage(&location.to_path(&archive_dir));

    // Open the archive again to avoid cache effects.
    let archive =
        Archive::open(conserve::transport::open_local_transport(archive_dir.path()).unwrap())
            .expect("open archive");

    // A second backup should succeed.
    changes.apply(&source_dir);
    let backup_stats = backup(
        &archive,
        source_dir.path(),
        &backup_options,
        CollectMonitor::arc(),
    )
    .expect("write second backup after damage");
    dbg!(&backup_stats);

    match changes {
        TreeChanges::None => match location {
            DamageLocation::Block(_) => {
                assert_eq!(backup_stats.replaced_damaged_blocks, 1);
                assert_eq!(backup_stats.written_blocks, 1);
            }
            _ => {
                assert_eq!(backup_stats.replaced_damaged_blocks, 0);
                assert_eq!(backup_stats.written_blocks, 0);
            }
        },
        TreeChanges::RenameFile => match location {
            DamageLocation::Block(_) => {
                // We can't deduplicate against the previous block because it's damaged.
                assert_eq!(backup_stats.written_blocks, 1);
                assert_eq!(backup_stats.replaced_damaged_blocks, 0);
            }
            _ => {
                // The file is renamed, but with the same content, so it should match the block from the previous backup.
                assert_eq!(backup_stats.deduplicated_blocks, 1);
            }
        },
        TreeChanges::AlterFile => {
            // a new block is written regardless
            assert_eq!(backup_stats.written_blocks, 1);
            assert_eq!(backup_stats.deduplicated_blocks, 0);
            assert_eq!(backup_stats.replaced_damaged_blocks, 0);
        }
    }

    // Can restore the second backup
    {
        let restore_dir = TempDir::new().unwrap();
        let monitor = CollectMonitor::arc();
        let restore_stats = restore(
            &archive,
            restore_dir.path(),
            &RestoreOptions::default(),
            monitor.clone(),
        )
        .expect("restore second backup");
        dbg!(&restore_stats);
        assert_eq!(restore_stats.files, 1);
        monitor.assert_no_errors();

        // Since the second backup rewrote the single file in the backup (and the root dir),
        // we should get all the content back out.
        assert_paths!(source_dir.path(), restore_dir.path());
    }

    // You can see both versions.
    let versions = archive.list_band_ids().expect("list versions");
    assert_eq!(versions, [BandId::zero(), BandId::new(&[1])]);

    // Can list the contents of the second backup.
    let apaths = archive
        .iter_entries(
            BandSelectionPolicy::Latest,
            Apath::root(),
            Exclude::nothing(),
        )
        .expect("iter entries")
        .map(|e| e.apath().to_string())
        .collect_vec();

    if matches!(changes, TreeChanges::RenameFile) {
        assert_eq!(apaths, ["/", "/file2"]);
    } else {
        assert_eq!(apaths, ["/", "/file"]);
    }

    // Validation completes although with warnings.
    // TODO: This should return problems that we can inspect.
    archive
        .validate(&ValidateOptions::default(), Arc::new(CollectMonitor::new()))
        .expect("validate");
}
