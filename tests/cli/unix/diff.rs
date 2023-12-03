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

//! Test `conserve diff` on Unix with symlinks.

use std::fs;

use assert_cmd::prelude::*;
use predicates::prelude::*;

use conserve::test_fixtures::{ScratchArchive, TreeFixture};

use crate::run_conserve;

fn setup_symlink() -> (ScratchArchive, TreeFixture) {
    let af = ScratchArchive::new();
    let tf = TreeFixture::new();
    tf.create_dir("subdir");
    tf.create_symlink("subdir/link", "target");
    run_conserve()
        .arg("backup")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success();
    (af, tf)
}

#[test]
pub fn symlink_unchanged() {
    let (af, tf) = setup_symlink();

    run_conserve()
        .arg("diff")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::is_empty());

    run_conserve()
        .arg("diff")
        .arg("--include-unchanged")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(". /\n. /subdir\n. /subdir/link\n")
        .stderr(predicate::str::is_empty());
}

#[test]
pub fn symlink_changed() {
    let (af, tf) = setup_symlink();
    fs::remove_file(tf.path().join("subdir/link")).unwrap();
    tf.create_symlink("subdir/link", "newtarget");

    run_conserve()
        .arg("diff")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout("* /subdir/link\n")
        .stderr(predicate::str::is_empty());

    run_conserve()
        .arg("diff")
        .arg("--include-unchanged")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(". /\n. /subdir\n* /subdir/link\n")
        .stderr(predicate::str::is_empty());
}
