// Conserve backup system.
// Copyright 2021 Martin Pool.

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
use predicates::function::function;

use crate::run_conserve;

#[test]
fn versions() {
    run_conserve()
        .args(&["versions", "--utc", "testdata/archive/simple/v0.6.10"])
        .assert()
        .success()
        .stdout(
            "\
b0000                complete   2021-03-04 13:21:15     0:00
b0001                complete   2021-03-04 13:21:30     0:00
b0002                complete   2021-03-04 13:27:28     0:00
",
        );
}

#[test]
fn versions_in_local_time() {
    // Without --utc we don't know exactly what times will be produced,
    // and it's hard to control the timezone for tests on Windows.
    run_conserve()
        .args(&["versions", "testdata/archive/simple/v0.6.10"])
        .assert()
        .success()
        .stdout(function(|s: &str| s.lines().count() == 3));
}

#[test]
fn versions_short() {
    run_conserve()
        .args(&["versions", "--short", "testdata/archive/simple/v0.6.10"])
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
fn versions_sizes() {
    run_conserve()
        .args(&[
            "versions",
            "--sizes",
            "--utc",
            "testdata/archive/simple/v0.6.10",
        ])
        .assert()
        .success()
        .stdout(
            "\
b0000                complete   2021-03-04 13:21:15     0:00           0 MB
b0001                complete   2021-03-04 13:21:30     0:00           0 MB
b0002                complete   2021-03-04 13:27:28     0:00           0 MB
",
        );
}
