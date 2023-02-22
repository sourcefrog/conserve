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

use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;

use conserve::test_fixtures::{ScratchArchive, TreeFixture};
use serde_json::Value;

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
    let new_rs_content = b"pub fn main() {}";
    tf.create_file_with_contents("src/new.rs", new_rs_content);

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

    // Inspect json diff
    let command = run_conserve()
        .args(["diff", "-j"])
        .arg(af.path())
        .arg(tf.path())
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let diff_json = &command.get_output().stdout;
    println!("{}", std::str::from_utf8(diff_json).unwrap());
    let diff = serde_json::Deserializer::from_slice(diff_json)
        .into_iter::<Value>()
        .collect::<Result<Vec<Value>, _>>()
        .unwrap();
    println!("{diff:#?}");
    assert_eq!(diff.len(), 2);
    assert_eq!(diff[0]["apath"], "/src");
    assert_eq!(diff[0]["added"]["kind"], "Dir");
    assert_eq!(diff[0]["added"]["size"], Value::Null);
    assert!(diff[0]["added"]["mtime"].is_string());
    assert!(diff[0]["added"]["user"].is_string());
    assert!(diff[0]["added"]["group"].is_string());
    assert_eq!(diff[1]["apath"], "/src/new.rs");
    assert_eq!(diff[1]["added"]["kind"], "File");
    assert_eq!(diff[1]["added"]["size"], new_rs_content.len());
    assert!(diff[1]["added"]["mtime"].is_string());
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
