// Conserve backup system.
// Copyright 2020, Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Read archives written by older versions.

use std::fs;
use std::path::Path;

use assert_fs::prelude::*;
use assert_fs::TempDir;
use copy_dir::copy_dir;
use predicates::prelude::*;

use conserve::archive::Archive;
use conserve::backup::BackupOptions;
use conserve::bandid::BandId;
use conserve::copy_tree::{copy_tree, CopyOptions};
use conserve::restore::RestoreTree;
use conserve::StoredTree;

const ARCHIVE_VERSIONS: &[&str] = &["0.6.0", "0.6.2", "0.6.3"];

fn open_old_archive(ver: &str, name: &str) -> Archive {
    Archive::open_path(&Path::new(&format!("testdata/archive/v{}/{}/", ver, name)))
        .expect("Failed to open archive")
}

#[test]
fn examine_archive() {
    for ver in ARCHIVE_VERSIONS {
        println!("examine {}", ver);
        let archive = open_old_archive(ver, "minimal-1");

        let band_ids = archive.list_band_ids().expect("Failed to list band ids");
        assert_eq!(band_ids, &[BandId::zero()]);

        assert_eq!(
            archive
                .last_band_id()
                .expect("Get last_band_id")
                .expect("Should have a last band id"),
            BandId::zero()
        );
    }
}

#[test]
fn validate_archive() {
    for ver in ARCHIVE_VERSIONS {
        println!("validate {}", ver);
        let archive = open_old_archive(ver, "minimal-1");

        let stats = archive.validate().expect("validate archive");
        assert_eq!(stats.structure_problems, 0);
        assert_eq!(stats.io_errors, 0);

        assert_eq!(stats.block_dir.block_error_count, 0);
    }
}

#[test]
fn restore_old_archive() {
    for ver in ARCHIVE_VERSIONS {
        let dest = TempDir::new().unwrap();
        println!("restore {} to {:?}", ver, dest.path());

        let archive = open_old_archive(ver, "minimal-1");
        // TODO(#123): Simpler backup/restore APIs.
        let st = StoredTree::open_last(&archive).expect("open last tree");
        let rt = RestoreTree::create(&dest.path()).expect("RestoreTree::create");

        let copy_stats = copy_tree(&st, rt, &CopyOptions::default()).expect("copy_tree");

        assert_eq!(copy_stats.files, 2);
        assert_eq!(copy_stats.symlinks, 0);
        assert_eq!(copy_stats.directories, 2);
        assert_eq!(copy_stats.errors, 0);
        assert_eq!(copy_stats.empty_files, 0);

        dest.child("hello").assert("hello world\n");
        dest.child("subdir").assert(predicate::path::is_dir());
        dest.child("subdir")
            .child("subfile")
            .assert("I like Rust\n");
    }
}

#[test]
fn restore_modify_backup() {
    for ver in ARCHIVE_VERSIONS {
        let working_tree = TempDir::new().unwrap();
        println!("restore {} to {:?}", ver, working_tree.path());

        let archive = open_old_archive(ver, "minimal-1");
        // TODO(#123): Simpler backup/restore APIs.
        let st = StoredTree::open_last(&archive).expect("open last tree");
        let rt = RestoreTree::create(&working_tree.path()).expect("RestoreTree::create");

        let _copy_stats = copy_tree(&st, rt, &CopyOptions::default()).expect("copy_tree");

        // Write back into a new copy of the archive, without modifying the
        // testdata in the source tree.
        let new_archive_temp = TempDir::new().unwrap();
        let stored_archive_path = format!("testdata/archive/v{}/minimal-1", ver);
        let new_archive_path = new_archive_temp.path().join("archive");
        copy_dir(stored_archive_path, &new_archive_path).expect("copy archive tree");

        working_tree
            .child("empty")
            .touch()
            .expect("Create empty file");
        fs::write(
            working_tree.path().join("subdir").join("subfile"),
            "I REALLY like Rust\n",
        )
        .expect("overwrite file");

        let new_archive = Archive::open_path(&new_archive_path).expect("Open new archive");
        let backup_stats = new_archive
            .backup(&working_tree.path(), &BackupOptions::default())
            .expect("Backup modified tree");

        assert_eq!(backup_stats.files, 3);
        // unmodified_files should be 0, but the unchanged file is s not defected as unmodified,
        // for two reasons.
        //
        // 1. We don't restore the mtime, so that doesn't match (#47).
        //
        // 2. Files that have a different mtime but the same content aren't seen as
        //    unmodified.  (And, maybe that's correct: we did have to do work to read and hash all
        //    the content. Perhaps they should be counted separately: #124.)
        //
        // Similarly, modified_files should be 1.
        assert_eq!(backup_stats.unmodified_files, 0);
        assert_eq!(backup_stats.modified_files, 2);
        assert_eq!(backup_stats.new_files, 1);
        assert_eq!(backup_stats.empty_files, 1);

        // The empty file doesn't use any blocks, and the unchanged file doesn't produce
        // any new blocks. So, just one for the genuinely new content.
        assert_eq!(backup_stats.written_blocks, 1);
        assert_eq!(backup_stats.errors, 0);

        working_tree.close().expect("Cleanup working tree");
        new_archive_temp.close().expect("Cleanup copied archive");
    }
}
