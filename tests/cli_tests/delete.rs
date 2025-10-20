// Conserve backup system.
// Copyright 2016-2025 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Test `conserve delete`.

use assert_cmd::prelude::*;
use assert_fs::TempDir;
use assert_fs::prelude::*;
use conserve::Archive;
use conserve::test_fixtures::store_two_versions;
use conserve::transport::Transport;
use predicates::prelude::*;

use conserve::BandId;

use crate::run_conserve;

#[tokio::test]
async fn delete_both_bands() {
    let temp = Transport::temp();
    let archive = Archive::create(temp.clone()).await.unwrap();
    store_two_versions(&archive).await;
    drop(archive);

    run_conserve()
        .args(["delete"])
        .args(["-b", "b0000"])
        .args(["-b", "b0001"])
        .arg(temp.local_path().unwrap())
        .assert()
        .success();

    let archive = Archive::open(temp.clone()).await.unwrap();
    assert_eq!(archive.list_band_ids().await.unwrap().len(), 0);
    assert_eq!(archive.all_blocks().await.unwrap().len(), 0);
}

#[tokio::test]
async fn delete_first_version() {
    let temp = Transport::temp();
    let archive = Archive::create(temp.clone()).await.unwrap();
    store_two_versions(&archive).await;
    drop(archive);

    run_conserve()
        .args(["delete"])
        .args(["-b", "b0"])
        .arg(temp.local_path().unwrap())
        .assert()
        .success();

    let archive = Archive::open(temp.clone()).await.unwrap();
    assert_eq!(archive.list_band_ids().await.unwrap(), &[BandId::new(&[1])]);
    // b0 contains two small files packed into the same block, which is not deleted.
    // b1 (not deleted) adds one additional block, which is still referenced.
    assert_eq!(archive.all_blocks().await.unwrap().len(), 2);

    let rd = TempDir::new().unwrap();
    run_conserve()
        .arg("restore")
        .arg(temp.local_path().unwrap())
        .arg(rd.path())
        .assert()
        .success();
    rd.child("hello").assert(predicate::path::is_file());
    rd.child("hello").assert(predicate::eq("contents"));
    rd.child("subdir/subfile").assert(predicate::eq("contents"));

    // File added in b1 has been restored.
    rd.child("hello2").assert(predicate::eq("contents"));

    run_conserve()
        .arg("validate")
        .arg(temp.local_path().unwrap())
        .assert()
        .success();
}

#[tokio::test]
async fn delete_second_version() {
    let temp = Transport::temp();
    let archive = Archive::create(temp.clone()).await.unwrap();
    store_two_versions(&archive).await;
    drop(archive);

    run_conserve()
        .args(["delete"])
        .args(["-b", "b1"])
        .arg(temp.local_path().unwrap())
        .assert()
        .success();

    let archive = Archive::open(temp.clone()).await.unwrap();
    assert_eq!(archive.list_band_ids().await.unwrap(), &[BandId::new(&[0])]);
    // b0 contains two small files packed into the same block.
    assert_eq!(archive.all_blocks().await.unwrap().len(), 1);

    let rd = TempDir::new().unwrap();
    run_conserve()
        .arg("restore")
        .arg(temp.local_path().unwrap())
        .arg(rd.path())
        .assert()
        .success();
    rd.child("hello").assert(predicate::path::is_file());
    rd.child("hello").assert(predicate::eq("contents"));
    rd.child("subdir/subfile").assert(predicate::eq("contents"));

    // File added in b1 should not have been restored.
    rd.child("hello2").assert(predicate::path::exists().not());

    run_conserve()
        .arg("validate")
        .arg(temp.local_path().unwrap())
        .assert()
        .success();
}

#[tokio::test]
async fn delete_nonexistent_band() {
    let temp = Transport::temp();
    let archive = Archive::create(temp.clone()).await.unwrap();
    drop(archive);

    run_conserve()
        .args(["delete"])
        .args(["-b", "b0000"])
        .arg(temp.local_path().unwrap())
        .assert()
        .stderr(predicate::str::contains(
            "ERROR conserve: Band not found: b0000",
        ))
        .failure();
}
