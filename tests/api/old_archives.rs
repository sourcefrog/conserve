// Conserve backup system.
// Copyright 2020, 2022 Martin Pool.

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

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::{self, metadata, read_dir};
use std::path::Path;

use assert_fs::prelude::*;
use assert_fs::TempDir;
use predicates::prelude::*;
use pretty_assertions::assert_eq;

use conserve::*;
use time::OffsetDateTime;
use tracing_test::traced_test;

const MINIMAL_ARCHIVE_VERSIONS: &[&str] = &["0.6.0", "0.6.10", "0.6.2", "0.6.3", "0.6.9", "0.6.17"];

fn open_old_archive(ver: &str, name: &str) -> Archive {
    Archive::open_path(Path::new(&archive_testdata_path(name, ver)))
        .expect("Failed to open archive")
}

fn archive_testdata_path(name: &str, ver: &str) -> String {
    format!("testdata/archive/{name}/v{ver}/")
}

#[test]
fn all_archive_versions_are_tested() {
    let present_subdirs: HashSet<String> = read_dir("testdata/archive/minimal")
        .unwrap()
        .map(|direntry| direntry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != ".gitattributes")
        .collect();
    assert_eq!(
        present_subdirs,
        MINIMAL_ARCHIVE_VERSIONS
            .iter()
            .map(|s| format!("v{s}"))
            .collect::<HashSet<String>>()
    );
}

#[test]
fn examine_archive() {
    for ver in MINIMAL_ARCHIVE_VERSIONS {
        println!("examine {ver}");
        let archive = open_old_archive(ver, "minimal");

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

#[traced_test]
#[test]
fn validate_archive() {
    for ver in MINIMAL_ARCHIVE_VERSIONS {
        println!("validate {ver}");
        let archive = open_old_archive(ver, "minimal");

        archive
            .validate(&ValidateOptions::default())
            .expect("validate archive");
        assert!(!logs_contain("ERROR") && !logs_contain("WARN"));
    }
}

#[test]
fn long_listing_old_archive() {
    let first_with_perms = semver::VersionReq::parse(">=0.6.17").unwrap();

    for ver in MINIMAL_ARCHIVE_VERSIONS {
        let dest = TempDir::new().unwrap();
        println!("restore {} to {:?}", ver, dest.path());

        let archive = open_old_archive(ver, "minimal");
        let mut stdout = Vec::<u8>::new();

        // show archive contents
        show::show_entry_names(
            archive
                .open_stored_tree(BandSelectionPolicy::Latest)
                .unwrap()
                .iter_entries(Apath::root(), Exclude::nothing())
                .unwrap(),
            &mut stdout,
            true,
        )
        .unwrap();

        if first_with_perms.matches(&semver::Version::parse(ver).unwrap()) {
            assert_eq!(
                String::from_utf8(stdout).unwrap(),
                "\
                    rwxrwxr-x mbp        mbp        /\n\
                    rw-rw-r-- mbp        mbp        /hello\n\
                    rwxrwxr-x mbp        mbp        /subdir\n\
                    rw-rw-r-- mbp        mbp        /subdir/subfile\n",
            );
        } else {
            assert_eq!(
                String::from_utf8(stdout).unwrap(),
                "\
                    none      none       none       /\n\
                    none      none       none       /hello\n\
                    none      none       none       /subdir\n\
                    none      none       none       /subdir/subfile\n",
            );
        }
    }
}

#[test]
fn restore_old_archive() {
    for ver in MINIMAL_ARCHIVE_VERSIONS {
        let dest = TempDir::new().unwrap();
        println!("restore {} to {:?}", ver, dest.path());

        let archive = open_old_archive(ver, "minimal");
        let restore_stats =
            restore(&archive, dest.path(), &RestoreOptions::default()).expect("restore");

        assert_eq!(restore_stats.files, 2);
        assert_eq!(restore_stats.symlinks, 0);
        assert_eq!(restore_stats.directories, 2);
        assert_eq!(restore_stats.errors, 0);

        dest.child("hello").assert("hello world\n");
        dest.child("subdir").assert(predicate::path::is_dir());
        dest.child("subdir")
            .child("subfile")
            .assert("I like Rust\n");

        // Check that mtimes are restored. The sub-second times are not tested
        // because their behavior might vary depending on the local filesystem.
        let file_mtime = OffsetDateTime::from(
            metadata(dest.child("hello").path())
                .unwrap()
                .modified()
                .unwrap(),
        );
        assert_eq!(
            file_mtime.unix_timestamp(),
            1592266523,
            "mtime not restored correctly"
        );

        let dir_mtime = OffsetDateTime::from(
            metadata(dest.child("subdir").path())
                .unwrap()
                .modified()
                .unwrap(),
        );
        assert_eq!(dir_mtime.unix_timestamp(), 1592266523);
    }
}

/// Restore from the old archive, modify the tree, then make a backup into a copy
/// of the old archive.
#[test]
fn restore_modify_backup() {
    for ver in MINIMAL_ARCHIVE_VERSIONS {
        let working_tree = TempDir::new().unwrap();
        println!("restore {} to {:?}", ver, working_tree.path());

        let archive = open_old_archive(ver, "minimal");

        restore(&archive, working_tree.path(), &RestoreOptions::default()).expect("restore");

        // Write back into a new copy of the archive, without modifying the
        // testdata in the source tree.
        let new_archive_temp = TempDir::new().unwrap();
        let stored_archive_path = archive_testdata_path("minimal", ver);
        let new_archive_path = new_archive_temp.path().join("archive");
        cp_r::CopyOptions::default()
            .copy_tree(stored_archive_path, &new_archive_path)
            .expect("copy archive tree");

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
        let emitted = RefCell::new(Vec::new());
        let backup_stats = backup(
            &new_archive,
            &LiveTree::open(working_tree.path()).unwrap(),
            &BackupOptions {
                after_entry: Some(Box::new(|change| {
                    emitted
                        .borrow_mut()
                        .push((change.diff_kind, change.apath.to_string()));
                    Ok(())
                })),
                ..Default::default()
            },
        )
        .expect("Backup modified tree");

        // Check the visited files passed to the callbacks.
        let emitted = emitted.into_inner();
        dbg!(&emitted);

        // Expected results for files:
        // "/empty" is empty and new
        // "/subdir/subfile" is modified
        // "/hello" is unmodified - but depending on the filesystem mtime resolution,
        // it might not be recognized as such.
        for path in &["empty", "subdir/subfile", "hello"] {
            println!(
                "{:<20} {:?}",
                path,
                working_tree.child(path).path().metadata().unwrap()
            );
        }
        assert!(emitted.contains(&(DiffKind::New, "/empty".to_owned())));
        assert!(emitted.contains(&(DiffKind::Changed, "/subdir/subfile".to_owned())));

        assert_eq!(backup_stats.files, 3);
        assert!(
            backup_stats.unmodified_files == 0 || backup_stats.unmodified_files == 1,
            "unmodified files"
        );
        assert!(
            backup_stats.modified_files == 1 || backup_stats.modified_files == 2,
            "modified files"
        );
        assert_eq!(
            backup_stats.modified_files + backup_stats.unmodified_files,
            2,
            "total modified & unmodified"
        );
        assert_eq!(backup_stats.new_files, 1, "new files");
        assert_eq!(backup_stats.empty_files, 1, "empty files");

        // The empty file doesn't use any blocks, and the unchanged file doesn't produce
        // any new blocks. So, just one for the genuinely new content.
        assert_eq!(backup_stats.written_blocks, 1);
        assert_eq!(backup_stats.errors, 0);

        working_tree.close().expect("Cleanup working tree");
        new_archive_temp.close().expect("Cleanup copied archive");
    }
}
