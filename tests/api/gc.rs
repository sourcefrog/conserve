// Copyright 2015, 2016, 2017, 2019, 2020, 2021 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Test garbage collection.

use conserve::test_fixtures::{ScratchArchive, TreeFixture};
use conserve::*;

#[test]
fn unreferenced_blocks() {
    let archive = ScratchArchive::new();
    let tf = TreeFixture::new();
    tf.create_file("hello");
    let content_hash: BlockHash =
        "9063990e5c5b2184877f92adace7c801a549b00c39cd7549877f06d5dd0d3a6ca6eee42d5\
        896bdac64831c8114c55cee664078bd105dc691270c92644ccb2ce7"
            .parse()
            .unwrap();

    let _copy_stats =
        backup(&archive, &tf.live_tree(), &BackupOptions::default(), None).expect("backup");

    // Delete the band and index
    std::fs::remove_dir_all(archive.path().join("b0000")).unwrap();

    let unreferenced: Vec<BlockHash> = archive.unreferenced_blocks(None).unwrap().collect();
    assert_eq!(unreferenced, [content_hash]);

    // Delete dry run.
    let delete_stats = archive
        .delete_bands(
            &[],
            &DeleteOptions {
                dry_run: true,
                break_lock: false,
            },
            None,
        )
        .unwrap();
    assert_eq!(
        delete_stats,
        DeleteStats {
            unreferenced_block_count: 1,
            unreferenced_block_bytes: 10,
            deletion_errors: 0,
            deleted_block_count: 0,
            deleted_band_count: 0,
            elapsed: delete_stats.elapsed,
        }
    );

    // Delete unreferenced blocks.
    let options = DeleteOptions {
        dry_run: false,
        break_lock: false,
    };
    let delete_stats = archive.delete_bands(&[], &options, None).unwrap();
    assert_eq!(
        delete_stats,
        DeleteStats {
            unreferenced_block_count: 1,
            unreferenced_block_bytes: 10,
            deletion_errors: 0,
            deleted_block_count: 1,
            deleted_band_count: 0,
            elapsed: delete_stats.elapsed,
        }
    );

    // Try again to delete: should find no garbage.
    let delete_stats = archive.delete_bands(&[], &options, None).unwrap();
    assert_eq!(
        delete_stats,
        DeleteStats {
            unreferenced_block_count: 0,
            unreferenced_block_bytes: 0,
            deletion_errors: 0,
            deleted_block_count: 0,
            deleted_band_count: 0,
            elapsed: delete_stats.elapsed,
        }
    );
}

#[test]
fn backup_prevented_by_gc_lock() -> Result<()> {
    let archive = ScratchArchive::new();
    let tf = TreeFixture::new();
    tf.create_file("hello");

    let lock1 = GarbageCollectionLock::new(&archive)?;

    // Backup should fail while gc lock is held.
    let backup_result = backup(&archive, &tf.live_tree(), &BackupOptions::default(), None);
    match backup_result {
        Err(Error::GarbageCollectionLockHeld) => (),
        other => panic!("unexpected result {other:?}"),
    };

    // Leak the lock, then gc breaking the lock.
    std::mem::forget(lock1);
    archive.delete_bands(
        &[],
        &DeleteOptions {
            break_lock: true,
            ..Default::default()
        },
        None,
    )?;

    // Backup should now succeed.
    let backup_result = backup(&archive, &tf.live_tree(), &BackupOptions::default(), None);
    assert!(backup_result.is_ok());

    Ok(())
}
