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

//! Tests for storing file ownership/user/group.

use conserve::backup::{backup, BackupOptions};
use conserve::monitor::collect::CollectMonitor;
use conserve::{restore, Archive, RestoreOptions};

use crate::copy_testdata_archive;

/// Adding owners to an archive where they are not already present
/// should not rewrite all the data.
///
/// More generally, changes to the size and mtime indicate that the
/// content needs to be read again, but a change of ownership does not.
///
/// <https://github.com/sourcefrog/conserve/issues/209>
#[test]
fn adding_owners_to_old_archive_does_not_rewrite_blocks() {
    // 0.6.0 didn't store owners, so we can use that as a base.
    let archive_temp = copy_testdata_archive("minimal", "0.6.0");
    let restore_temp = tempfile::tempdir().expect("create temp dir");
    let archive = Archive::open_path(archive_temp.path()).expect("open archive");
    let restore_monitor = CollectMonitor::arc();
    restore(
        &archive,
        restore_temp.path(),
        &RestoreOptions::default(),
        restore_monitor,
    )
    .expect("restore");

    // Now backup again without making any changes.
    let backup_monitor = CollectMonitor::arc();
    let stats = backup(
        &archive,
        restore_temp.path(),
        &BackupOptions::default(),
        backup_monitor,
    )
    .expect("backup");

    // We should have written new index hunks with owners, but not
    // data blocks.
    assert_eq!(stats.written_blocks, 0);
}
