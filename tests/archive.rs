// Conserve backup system.
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

//! API tests for archives.

use std::fs;
use std::io::Read;

use assert_fs::prelude::*;
use assert_fs::TempDir;

use conserve::archive::Archive;
use conserve::monitor::test::TestMonitor;

#[tokio::test]
async fn create_then_open_archive() {
    let testdir = TempDir::new().unwrap();
    let arch_path = testdir.path().join("arch");
    let arch = Archive::create_path(&arch_path).await.unwrap();

    assert!(arch.list_band_ids().await.unwrap().is_empty());

    // We can re-open it.
    Archive::open_path(&arch_path).await.unwrap();
    assert!(arch.list_band_ids().await.unwrap().is_empty());
    assert!(arch.last_complete_band().await.unwrap().is_none());
}

#[tokio::test]
async fn fails_on_non_empty_directory() {
    let temp = TempDir::new().unwrap();

    temp.child("i am already here").touch().unwrap();

    let result = Archive::create_path(temp.path()).await;
    assert_eq!(
        result.as_ref().unwrap_err().to_string(),
        "Directory for new archive is not empty",
        "{result:?}"
    );
    temp.close().unwrap();
}

/// A new archive contains just one header file.
/// The header is readable json containing only a version number.
#[tokio::test]
async fn empty_archive() {
    let af = Archive::create_temp().await;

    assert!(af.transport().local_path().unwrap().is_dir());
    assert!(af
        .transport()
        .local_path()
        .unwrap()
        .join("CONSERVE")
        .is_file());
    assert!(af.transport().local_path().unwrap().join("d").is_dir());

    let header_path = af.transport().local_path().unwrap().join("CONSERVE");
    let mut header_file = fs::File::open(header_path).unwrap();
    let mut contents = String::new();
    header_file.read_to_string(&mut contents).unwrap();
    assert_eq!(contents, "{\"conserve_archive_version\":\"0.6\"}\n");

    assert!(
        af.last_band_id().await.unwrap().is_none(),
        "Archive should have no bands yet"
    );
    assert!(
        af.last_complete_band().await.unwrap().is_none(),
        "Archive should have no bands yet"
    );
    assert_eq!(
        af.referenced_blocks(&af.list_band_ids().await.unwrap(), TestMonitor::arc())
            .await
            .unwrap()
            .len(),
        0
    );
    assert_eq!(af.all_blocks(TestMonitor::arc()).await.unwrap().len(), 0);
}
