// Conserve backup system.
// Copyright 2016, 2017, 2018, 2019, 2020 Martin Pool.

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
use predicates::prelude::*;

use conserve::test_fixtures::ScratchArchive;

use crate::run_conserve;

#[test]
fn delete_bands() {
    let af = ScratchArchive::new();
    af.store_two_versions();

    run_conserve()
        .args(&["delete"])
        .args(&["-b", "b0000"])
        .args(&["-b", "b0001"])
        .arg(af.path())
        .assert()
        .success();
}

#[test]
fn delete_nonexistent_band() {
    let af = ScratchArchive::new();

    let pred_fn = predicate::str::is_match(
        r"conserve error: Failed to delete band b0000
  caused by: (No such file or directory|The system cannot find the file specified\.) \(os error \d+\)
",
        )
        .unwrap();

    run_conserve()
        .args(&["delete"])
        .args(&["-b", "b0000"])
        .arg(af.path())
        .assert()
        .stdout(pred_fn)
        .failure();
}
