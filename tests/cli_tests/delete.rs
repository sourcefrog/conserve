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
use assert_fs::prelude::*;
use assert_fs::TempDir;
use conserve::monitor::test::TestMonitor;
use conserve::test_fixtures::store_two_versions;
use conserve::Archive;
use predicates::prelude::*;

use conserve::BandId;

use crate::run_conserve;

#[tokio::test]
async fn delete_both_bands() {
    let af = Archive::create_temp().await;
    store_two_versions(&af).await;

    run_conserve()
        .args(["delete"])
        .args(["-b", "b0000"])
        .args(["-b", "b0001"])
        .arg(af.transport().local_path().unwrap())
        .assert()
        .success();

    assert_eq!(af.list_band_ids().await.unwrap().len(), 0);
    assert_eq!(af.all_blocks(TestMonitor::arc()).await.unwrap().len(), 0);
}

#[tokio::test]
async fn delete_first_version() {
    let af = Archive::create_temp().await;
    store_two_versions(&af).await;

    run_conserve()
        .args(["delete"])
        .args(["-b", "b0"])
        .arg(af.transport().local_path().unwrap())
        .assert()
        .success();

    assert_eq!(af.list_band_ids().await.unwrap(), &[BandId::new(&[1])]);
    // b0 contains two small files packed into the same block, which is not deleted.
    // b1 (not deleted) adds one additional block, which is still referenced.
    assert_eq!(af.all_blocks(TestMonitor::arc()).await.unwrap().len(), 2);

    let rd = TempDir::new().unwrap();
    run_conserve()
        .arg("restore")
        .arg(af.transport().local_path().unwrap())
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
        .arg(af.transport().local_path().unwrap())
        .assert()
        .success();
}

#[tokio::test]
async fn delete_second_version() {
    let af = Archive::create_temp().await;
    store_two_versions(&af).await;

    run_conserve()
        .args(["delete"])
        .args(["-b", "b1"])
        .arg(af.transport().local_path().unwrap())
        .assert()
        .success();

    assert_eq!(af.list_band_ids().await.unwrap(), &[BandId::new(&[0])]);
    // b0 contains two small files packed into the same block.
    assert_eq!(af.all_blocks(TestMonitor::arc()).await.unwrap().len(), 1);

    let rd = TempDir::new().unwrap();
    run_conserve()
        .arg("restore")
        .arg(af.transport().local_path().unwrap())
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
        .arg(af.transport().local_path().unwrap())
        .assert()
        .success();
}

#[tokio::test]
async fn delete_nonexistent_band() {
    let af = Archive::create_temp().await;

    run_conserve()
        .args(["delete"])
        .args(["-b", "b0000"])
        .arg(af.transport().local_path().unwrap())
        .assert()
        .stderr(predicate::str::contains(
            "ERROR conserve: Band not found: b0000",
        ))
        .failure();
}
