// Copyright 2015-2025 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Tests focused on restore.

use std::fs::{create_dir, write};
use std::sync::{Arc, Mutex};

use conserve::counters::Counter;
use conserve::monitor::test::TestMonitor;
use tempfile::TempDir;

use conserve::test_fixtures::ScratchArchive;
use conserve::test_fixtures::TreeFixture;
use conserve::*;

#[tokio::test]
async fn simple_restore() {
    let af = ScratchArchive::new();
    af.store_two_versions().await;
    let destdir = TreeFixture::new();
    let restore_archive = Archive::open_path(af.path()).unwrap();
    let restored_names = Arc::new(Mutex::new(Vec::new()));
    let restored_names_clone = restored_names.clone();
    let options = RestoreOptions {
        change_callback: Some(Box::new(move |entry_change| {
            restored_names_clone
                .lock()
                .unwrap()
                .push(entry_change.apath.clone());
            Ok(())
        })),
        ..Default::default()
    };
    let monitor = TestMonitor::arc();
    restore(&restore_archive, destdir.path(), options, monitor.clone())
        .await
        .expect("restore");

    monitor.assert_no_errors();
    monitor.assert_counter(Counter::Files, 3);
    let mut expected_names = vec![
        "/",
        "/hello",
        "/hello2",
        "/link",
        "/subdir",
        "/subdir/subfile",
    ];
    if !SYMLINKS_SUPPORTED {
        expected_names.retain(|n| *n != "/link");
    }
    assert_eq!(restored_names.lock().unwrap().as_slice(), expected_names);

    let dest = &destdir.path();
    assert!(dest.join("hello").is_file());
    assert!(dest.join("hello2").is_file());
    assert!(dest.join("subdir").is_dir());
    assert!(dest.join("subdir").join("subfile").is_file());
    if SYMLINKS_SUPPORTED {
        let dest = std::fs::read_link(dest.join("link")).unwrap();
        assert_eq!(dest.to_string_lossy(), "target");
    }

    // TODO: Test file contents are as expected.
}

#[tokio::test]
async fn restore_specified_band() {
    let af = ScratchArchive::new();
    af.store_two_versions().await;
    let destdir = TreeFixture::new();
    let archive = Archive::open_path(af.path()).unwrap();
    let band_id = BandId::new(&[0]);
    let options = RestoreOptions {
        band_selection: BandSelectionPolicy::Specified(band_id),
        ..RestoreOptions::default()
    };
    let monitor = TestMonitor::arc();
    restore(&archive, destdir.path(), options, monitor.clone())
        .await
        .expect("restore");
    monitor.assert_no_errors();
    // Does not have the 'hello2' file added in the second version.
    monitor.assert_counter(Counter::Files, 2);
}

/// Restoring a subdirectory works, and restores the parent directories:
///
/// <https://github.com/sourcefrog/conserve/issues/268>
#[tokio::test]
async fn restore_only_subdir() {
    // We need the selected directory to be more than one level down, because the bug was that
    // its parent was not created.
    let backup_monitor = TestMonitor::arc();
    let src = TempDir::new().unwrap();
    create_dir(src.path().join("parent")).unwrap();
    create_dir(src.path().join("parent/sub")).unwrap();
    write(src.path().join("parent/sub/file"), b"hello").unwrap();
    let af = ScratchArchive::new();
    backup(
        &af,
        src.path(),
        &BackupOptions::default(),
        backup_monitor.clone(),
    )
    .await
    .unwrap();
    backup_monitor.assert_counter(Counter::Files, 1);
    backup_monitor.assert_no_errors();

    let destdir = TreeFixture::new();
    let restore_monitor = TestMonitor::arc();
    let archive = Archive::open_path(af.path()).unwrap();
    let options = RestoreOptions {
        only_subtree: Some(Apath::from("/parent/sub")),
        ..Default::default()
    };
    restore(&archive, destdir.path(), options, restore_monitor.clone())
        .await
        .expect("restore");
    restore_monitor.assert_no_errors();
    assert!(destdir.path().join("parent").is_dir());
    assert!(destdir.path().join("parent/sub/file").is_file());
    dbg!(restore_monitor.counters());
    restore_monitor.assert_counter(Counter::Files, 1);
}

#[tokio::test]
async fn decline_to_overwrite() {
    let af = ScratchArchive::new();
    af.store_two_versions().await;
    let destdir = TreeFixture::new();
    destdir.create_file("existing");
    let options = RestoreOptions {
        ..RestoreOptions::default()
    };
    assert!(!options.overwrite, "overwrite is false by default");
    let restore_err_str = restore(&af, destdir.path(), options, TestMonitor::arc())
        .await
        .expect_err("restore should fail if the destination exists")
        .to_string();
    assert!(
        restore_err_str.contains("Destination directory is not empty"),
        "Unexpected error message: {restore_err_str:?}"
    );
}

#[tokio::test]
async fn forced_overwrite() {
    let af = ScratchArchive::new();
    af.store_two_versions().await;
    let destdir = TreeFixture::new();
    destdir.create_file("existing");

    let restore_archive = Archive::open_path(af.path()).unwrap();
    let options = RestoreOptions {
        overwrite: true,
        ..RestoreOptions::default()
    };
    let monitor = TestMonitor::arc();
    restore(&restore_archive, destdir.path(), options, monitor.clone())
        .await
        .expect("restore");
    monitor.assert_no_errors();
    monitor.assert_counter(Counter::Files, 3);
    let dest = destdir.path();
    assert!(dest.join("hello").is_file());
    assert!(dest.join("existing").is_file());
}

#[tokio::test]
async fn exclude_files() {
    let af = ScratchArchive::new();
    af.store_two_versions().await;
    let destdir = TreeFixture::new();
    let restore_archive = Archive::open_path(af.path()).unwrap();
    let options = RestoreOptions {
        overwrite: true,
        exclude: Exclude::from_strings(["/**/subfile"]).unwrap(),
        ..RestoreOptions::default()
    };
    let monitor = TestMonitor::arc();
    restore(&restore_archive, destdir.path(), options, monitor.clone())
        .await
        .expect("restore");

    let dest = destdir.path();
    assert!(dest.join("hello").is_file());
    assert!(dest.join("hello2").is_file());
    assert!(dest.join("subdir").is_dir());
    monitor.assert_no_errors();
    monitor.assert_counter(Counter::Files, 2);
}

#[tokio::test]
#[cfg(unix)]
async fn restore_symlink() {
    use std::fs::{read_link, symlink_metadata};
    use std::path::PathBuf;

    use filetime::{set_symlink_file_times, FileTime};

    use conserve::monitor::test::TestMonitor;

    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();

    srcdir.create_symlink("symlink", "target");
    let years_ago = FileTime::from_unix_time(189216000, 0);
    set_symlink_file_times(srcdir.path().join("symlink"), years_ago, years_ago).unwrap();

    let monitor = TestMonitor::arc();
    backup(&af, srcdir.path(), &Default::default(), monitor.clone())
        .await
        .unwrap();

    let restore_dir = TempDir::new().unwrap();
    let monitor = TestMonitor::arc();
    restore(&af, restore_dir.path(), Default::default(), monitor.clone())
        .await
        .unwrap();

    let restored_symlink_path = restore_dir.path().join("symlink");
    let sym_meta = symlink_metadata(&restored_symlink_path).unwrap();
    assert!(sym_meta.file_type().is_symlink());
    assert_eq!(FileTime::from(sym_meta.modified().unwrap()), years_ago);
    assert_eq!(
        read_link(&restored_symlink_path).unwrap(),
        PathBuf::from("target")
    );
}
