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

//! Tests focussed on restore.

use std::fs::{read_link, symlink_metadata};
use std::path::PathBuf;

use filetime::{set_symlink_file_times, FileTime};
use tempfile::TempDir;

use conserve::test_fixtures::ScratchArchive;
use conserve::test_fixtures::TreeFixture;
use conserve::unix_time::UnixTime;
use conserve::*;

#[test]
#[cfg(unix)]
fn restore_symlink() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();

    srcdir.create_symlink("symlink", "target");
    let years_ago = UnixTime {
        secs: 189216000,
        nanosecs: 0,
    };
    let mtime: FileTime = years_ago.into();
    set_symlink_file_times(&srcdir.path().join("symlink"), mtime, mtime).unwrap();

    backup(&af, &srcdir.live_tree(), &Default::default()).unwrap();

    let restore_dir = TempDir::new().unwrap();
    af.restore(&restore_dir.path(), &Default::default())
        .unwrap();

    let restored_symlink_path = restore_dir.path().join("symlink");
    let sym_meta = symlink_metadata(&restored_symlink_path).unwrap();
    assert!(sym_meta.file_type().is_symlink());
    assert_eq!(UnixTime::from(sym_meta.modified().unwrap()), years_ago);
    assert_eq!(
        read_link(&restored_symlink_path).unwrap(),
        PathBuf::from("target")
    );
}
