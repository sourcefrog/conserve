// Conserve backup system.
// Copyright 2020-2025 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

/*!
 * Damage tests
 *
 * Conserve tries to still allow the archive to be read, and future backups to be written,
 * even if some files are damaged: truncated, corrupt, missing, or unreadable.
 *
 * This is not yet achieved in every case, but the format and code are designed to
 * work towards this goal.
 *
 * These API tests write an archive, create some damage, and then try to read other
 * information, write future backups, and validate.
 *
 * These are implemented as API tests for the sake of execution speed and ease of examining the results.
 *
 * "Damage strategies" are a combination of a "damage action" (which could be deleting or
 * truncating a file) and a "damage location" which selects the file to damage.
 */

use std::fs::rename;
use std::fs::{remove_file, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use assert_fs::prelude::*;
use assert_fs::TempDir;
use dir_assert::assert_paths;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use rstest::rstest;
use tracing::info;
use tracing_test::traced_test;
// use predicates::prelude::*;

use conserve::counters::Counter;
use conserve::monitor::test::TestMonitor;
use conserve::transport::Transport;
use conserve::{
    backup, restore, Apath, Archive, BackupOptions, BandId, BandSelectionPolicy, EntryTrait,
    Exclude, RestoreOptions, ValidateOptions,
};

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
#[tokio::test] async
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
        TestMonitor::arc(),
    )
    .expect("initial backup");

    drop(archive);
    action.damage(&location.to_path(&archive_dir).await);

    // Open the archive again to avoid cache effects.
    let archive = Archive::open(Transport::local(archive_dir.path())).expect("open archive");

    // A second backup should succeed.
    changes.apply(&source_dir);
    let backup_stats = backup(
        &archive,
        source_dir.path(),
        &backup_options,
        TestMonitor::arc(),
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
        let monitor = TestMonitor::arc();
        restore(
            &archive,
            restore_dir.path(),
            &RestoreOptions::default(),
            monitor.clone(),
        )
        .expect("restore second backup");
        monitor.assert_counter(Counter::Files, 1);
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
            TestMonitor::arc(),
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
        .validate(&ValidateOptions::default(), Arc::new(TestMonitor::new()))
        .expect("validate");
}

/// A way of damaging a file in an archive.
#[derive(Debug, Clone)]
pub enum DamageAction {
    /// Truncate the file to zero bytes.
    Truncate,

    /// Delete the file.
    Delete,
    // TODO: Also test other types of damage, including
    // permission denied (as a kind of IOError), and binary junk.
}

impl DamageAction {
    /// Apply this damage to a file.
    ///
    /// The file must already exist.
    pub fn damage(&self, path: &Path) {
        info!(?self, ?path, "Apply damage!");
        assert!(path.exists(), "Path to be damaged does not exist: {path:?}");
        match self {
            DamageAction::Truncate => {
                OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(path)
                    .expect("truncate file");
            }
            DamageAction::Delete => {
                remove_file(path).expect("delete file");
            }
        }
    }
}

/// An abstract description of which file will be damaged.
///
/// Bands are identified by untyped integers for brevity in rstest names.
#[derive(Debug, Clone)]
pub enum DamageLocation {
    /// Delete the head of a band.
    BandHead(u32),
    BandTail(u32),
    /// Damage a block, identified by its index in the sorted list of all blocks in the archive,
    /// to avoid needing to hardcode a hash in the test.
    Block(usize),
    // TODO: Also test damage to other files: index hunks, archive header, etc.
}

impl DamageLocation {
    /// Find the specific path for this location, within an archive.
    async fn to_path(&self, archive_dir: &Path) -> PathBuf {
        match self {
            DamageLocation::BandHead(band_id) => archive_dir
                .join(BandId::from(*band_id).to_string())
                .join("BANDHEAD"),
            DamageLocation::BandTail(band_id) => archive_dir
                .join(BandId::from(*band_id).to_string())
                .join("BANDTAIL"),
            DamageLocation::Block(block_index) => {
                let archive = Archive::open(Transport::local(archive_dir)).expect("open archive");
                let block_hash = archive
                    .all_blocks(TestMonitor::arc())
                    .await
                    .expect("list blocks")
                    .into_iter()
                    .sorted()
                    .nth(*block_index)
                    .expect("Archive has an nth block");
                archive_dir
                    .join("d")
                    .join(conserve::blockdir::block_relpath(&block_hash))
            }
        }
    }
}
