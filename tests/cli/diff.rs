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

#[test]
fn add_entries() {
    let (af, tf) = setup();
    tf.create_dir("src");
    tf.create_file_with_contents("src/new.rs", b"pub fn main() {}");

    run_conserve()
        .arg("diff")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout("+\t/src\n+\t/src/new.rs\n")
        .stderr(predicate::str::is_empty());
}

#[test]
fn remove_file() {
    let (af, tf) = setup();
    std::fs::remove_file(tf.path().join("hello.c")).unwrap();

    run_conserve()
        .arg("diff")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout("-\t/hello.c\n")
        .stderr(predicate::str::is_empty());

    run_conserve()
        .arg("diff")
        .arg("--include-unchanged")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(".\t/\n-\t/hello.c\n.\t/subdir\n")
        .stderr(predicate::str::is_empty());
}

fn setup() -> (ScratchArchive, TreeFixture) {
    let af = ScratchArchive::new();
    let tf = TreeFixture::new();
    tf.create_file_with_contents("hello.c", b"void main() {}");
    tf.create_dir("subdir");
    run_conserve()
        .arg("backup")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success();
    (af, tf)
}
