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

//! Test `conserve diff`.

use assert_cmd::prelude::*;
use predicates::prelude::*;

use conserve::test_fixtures::{ScratchArchive, TreeFixture};

use crate::run_conserve;

#[test]
fn no_changes() {
    let (af, tf) = setup();
    run_conserve()
        .arg("backup")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success();

    run_conserve()
        .arg("diff")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());

    run_conserve()
        .arg("diff")
        .arg("--include-unchanged")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(".\t/\n.\t/hello.c\n.\t/subdir\n")
        .stderr(predicate::str::is_empty());
}

fn setup() -> (ScratchArchive, TreeFixture) {
    let af = ScratchArchive::new();
    let tf = TreeFixture::new();
    tf.create_file_with_contents("hello.c", b"void main() {}");
    tf.create_dir("subdir");
    (af, tf)
}
