// Conserve backup system.
// Copyright 2021-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Tests of the `conserve versions` command.

use assert_cmd::prelude::*;
use conserve::Archive;
use indoc::indoc;
use predicates::function::function;
use predicates::prelude::*;

use crate::run_conserve;

#[test]
fn utc() {
    run_conserve()
        .args(["versions", "--utc", "testdata/archive/simple/v0.6.10"])
        .assert()
        .success()
        .stdout(indoc! { "
            b0000                2021-03-04T13:21:15Z            0:00
            b0001                2021-03-04T13:21:30Z            0:00
            b0002                2021-03-04T13:27:28Z            0:00
            "});
}

#[test]
fn newest_first() {
    run_conserve()
        .args([
            "versions",
            "--newest",
            "--utc",
            "testdata/archive/simple/v0.6.10",
        ])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(indoc! { "
            b0002                2021-03-04T13:27:28Z            0:00
            b0001                2021-03-04T13:21:30Z            0:00
            b0000                2021-03-04T13:21:15Z            0:00
        "});
}

#[test]
fn local_time() {
    // Without --utc we don't know exactly what times will be produced,
    // and it's hard to control the timezone for tests on Windows.
    run_conserve()
        .args(["versions", "testdata/archive/simple/v0.6.10"])
        .assert()
        .success()
        .stdout(function(|s: &str| s.lines().count() == 3));
}

#[test]
fn short() {
    run_conserve()
        .args(["versions", "--short", "testdata/archive/simple/v0.6.10"])
        .assert()
        .success()
        .stdout(
            "\
b0000
b0001
b0002
",
        );
}

#[test]
fn tree_sizes() {
    run_conserve()
        .args([
            "versions",
            "--sizes",
            "--utc",
            "testdata/archive/simple/v0.6.10",
        ])
        .assert()
        .success()
        .stdout(indoc! { "
            b0000                2021-03-04T13:21:15Z            0:00           0 MB
            b0001                2021-03-04T13:21:30Z            0:00           0 MB
            b0002                2021-03-04T13:27:28Z            0:00           0 MB
            "});
}

#[tokio::test]
async fn versions_short_newest_first() {
    let af = Archive::create_temp().await;
    conserve::test_fixtures::store_two_versions(&af).await;

    run_conserve()
        .args(["versions", "--short", "--newest"])
        .arg(af.transport().local_path().unwrap())
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout("b0001\nb0000\n");
}
