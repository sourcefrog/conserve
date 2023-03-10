// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Tests focussed on restore.

use std::cell::RefCell;
#[cfg(unix)]
use std::fs::{read_link, symlink_metadata};
use std::path::PathBuf;

use filetime::{set_symlink_file_times, FileTime};
use tempfile::TempDir;

use conserve::test_fixtures::ScratchArchive;
use conserve::test_fixtures::TreeFixture;
use conserve::*;

#[test]
fn simple_restore() {
    let af = ScratchArchive::new();
    af.store_two_versions();
    let destdir = TreeFixture::new();
    let restore_archive = Archive::open_path(af.path()).unwrap();
    let restored_names = RefCell::new(Vec::new());
    let options = RestoreOptions {
        change_callback: Some(Box::new(|entry_change| {
            restored_names.borrow_mut().push(entry_change.apath.clone());
            Ok(())
        })),
        ..Default::default()
    };
    let stats = restore(&restore_archive, destdir.path(), &options).expect("restore");

    assert_eq!(stats.files, 3);
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
    drop(options);
    assert_eq!(restored_names.into_inner(), expected_names);

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

#[test]
fn restore_specified_band() {
    let af = ScratchArchive::new();
    af.store_two_versions();
    let destdir = TreeFixture::new();
    let archive = Archive::open_path(af.path()).unwrap();
    let band_id = BandId::new(&[0]);
    let options = RestoreOptions {
        band_selection: BandSelectionPolicy::Specified(band_id),
        ..RestoreOptions::default()
    };
    let stats = restore(&archive, destdir.path(), &options).expect("restore");
    // Does not have the 'hello2' file added in the second version.
    assert_eq!(stats.files, 2);
}

#[test]
pub fn decline_to_overwrite() {
    let af = ScratchArchive::new();
    af.store_two_versions();
    let destdir = TreeFixture::new();
    destdir.create_file("existing");
    let options = RestoreOptions {
        ..RestoreOptions::default()
    };
    assert!(!options.overwrite, "overwrite is false by default");
    let restore_err_str = restore(&af, destdir.path(), &options)
        .expect_err("restore should fail if the destination exists")
        .to_string();
    assert!(restore_err_str.contains("Destination directory not empty"));
}

#[test]
pub fn forced_overwrite() {
    let af = ScratchArchive::new();
    af.store_two_versions();
    let destdir = TreeFixture::new();
    destdir.create_file("existing");

    let restore_archive = Archive::open_path(af.path()).unwrap();
    let options = RestoreOptions {
        overwrite: true,
        ..RestoreOptions::default()
    };
    let stats = restore(&restore_archive, destdir.path(), &options).expect("restore");
    assert_eq!(stats.files, 3);
    let dest = &destdir.path();
    assert!(dest.join("hello").is_file());
    assert!(dest.join("existing").is_file());
}

#[test]
fn exclude_files() {
    let af = ScratchArchive::new();
    af.store_two_versions();
    let destdir = TreeFixture::new();
    let restore_archive = Archive::open_path(af.path()).unwrap();
    let options = RestoreOptions {
        overwrite: true,
        exclude: Exclude::from_strings(["/**/subfile"]).unwrap(),
        ..RestoreOptions::default()
    };
    let stats = restore(&restore_archive, destdir.path(), &options).expect("restore");

    let dest = &destdir.path();
    assert!(dest.join("hello").is_file());
    assert!(dest.join("hello2").is_file());
    assert!(dest.join("subdir").is_dir());
    assert_eq!(stats.files, 2);
}

#[test]
#[cfg(unix)]
fn restore_symlink() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();

    srcdir.create_symlink("symlink", "target");
    let years_ago = FileTime::from_unix_time(189216000, 0);
    set_symlink_file_times(srcdir.path().join("symlink"), years_ago, years_ago).unwrap();

    backup(&af, &srcdir.live_tree(), &Default::default()).unwrap();

    let restore_dir = TempDir::new().unwrap();
    restore(&af, restore_dir.path(), &Default::default()).unwrap();

    let restored_symlink_path = restore_dir.path().join("symlink");
    let sym_meta = symlink_metadata(&restored_symlink_path).unwrap();
    assert!(sym_meta.file_type().is_symlink());
    assert_eq!(FileTime::from(sym_meta.modified().unwrap()), years_ago);
    assert_eq!(
        read_link(&restored_symlink_path).unwrap(),
        PathBuf::from("target")
    );
}
