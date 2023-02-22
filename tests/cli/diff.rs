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

//! Test `conserve diff`.

use std::fs;

use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;

use conserve::test_fixtures::{ScratchArchive, TreeFixture};

use crate::run_conserve;

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

#[cfg(unix)]
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
        .stdout(". /\n. /hello.c\n. /subdir\n")
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
        .stdout(indoc! {"
            + /src
            + /src/new.rs
        "})
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
        .stdout("- /hello.c\n")
        .stderr(predicate::str::is_empty());

    run_conserve()
        .arg("diff")
        .arg("--include-unchanged")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(". /\n- /hello.c\n. /subdir\n")
        .stderr(predicate::str::is_empty());
}

#[test]
fn change_kind() {
    let (af, tf) = setup();
    std::fs::remove_dir(tf.path().join("subdir")).unwrap();
    tf.create_file_with_contents("subdir", b"used to be a directory, no longer");

    run_conserve()
        .arg("diff")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(indoc! {"
            * /subdir
        "})
        .stderr(predicate::str::is_empty());

    run_conserve()
        .arg("diff")
        .arg("--include-unchanged")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(indoc! {"
            . /
            . /hello.c
            * /subdir
            "})
        .stderr(predicate::str::is_empty());
}

#[test]
fn change_file_content() {
    // This actually detects that the file size/mtime changed, and does not thoroughly read the file.
    let (af, tf) = setup();
    tf.create_file_with_contents("hello.c", b"int main() { abort(); }");

    run_conserve()
        .arg("diff")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(indoc! {"
            * /hello.c
            "})
        .stderr(predicate::str::is_empty());

    run_conserve()
        .arg("diff")
        .arg("--include-unchanged")
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stdout(indoc! {"
            . /
            * /hello.c
            . /subdir
            "})
        .stderr(predicate::str::is_empty());
}

#[cfg(unix)]
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

#[cfg(unix)]
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
