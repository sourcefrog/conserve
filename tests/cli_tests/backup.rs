// Conserve backup system.
// Copyright 2016-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use std::fs::read_to_string;

use assert_cmd::prelude::*;
use assert_fs::NamedTempFile;
use indoc::indoc;
use serde_json::Deserializer;

use conserve::{Archive, test_fixtures::TreeFixture};

use crate::run_conserve;

#[tokio::test]
async fn backup_verbose() {
    let af = Archive::create_temp().await;
    let src = TreeFixture::new();
    src.create_dir("subdir");
    src.create_file("subdir/a");
    src.create_file("subdir/b");
    let changes_json = NamedTempFile::new("changes.json").unwrap();
    let counters_tmp = NamedTempFile::new("counters.json").unwrap();

    run_conserve()
        .args(["backup", "--no-stats", "-v"])
        .arg(af.transport().local_path().unwrap())
        .arg(src.path())
        .arg("--changes-json")
        .arg(changes_json.path())
        .arg("--metrics-json")
        .arg(counters_tmp.path())
        .assert()
        .success()
        .stdout(indoc! { "
            + /subdir/a
            + /subdir/b
        "});

    let changes_json = read_to_string(changes_json.path()).unwrap();
    println!("{changes_json}");
    let changes: Vec<serde_json::Value> = Deserializer::from_str(&changes_json)
        .into_iter::<serde_json::Value>()
        .map(Result::unwrap)
        .collect();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["apath"], "/subdir/a");
    assert_eq!(changes[0]["change"], "Added");
    assert_eq!(changes[0]["added"]["kind"], "File");
    assert_eq!(changes[1]["apath"], "/subdir/b");
    assert_eq!(changes[1]["change"], "Added");
    assert_eq!(changes[1]["added"]["kind"], "File");

    let counters_json = read_to_string(counters_tmp).unwrap();
    println!("{counters_json}");
    let counters: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&counters_json).unwrap();
    assert_eq!(counters.get("Files").unwrap(), 2);
    assert_eq!(counters.get("Dirs").unwrap(), 2);
}

#[tokio::test]
async fn verbose_backup_does_not_print_unchanged_files() {
    let af = Archive::create_temp().await;
    let src = TreeFixture::new();
    src.create_file("a");
    src.create_file("b");

    run_conserve()
        .args(["backup", "--no-stats", "-v"])
        .arg(af.transport().local_path().unwrap())
        .arg(src.path())
        .assert()
        .success()
        .stdout(indoc! { "
            + /a
            + /b
        "});

    src.create_file_with_contents("b", b"new b contents");

    run_conserve()
        .args(["backup", "--no-stats", "-v"])
        .arg(af.transport().local_path().unwrap())
        .arg(src.path())
        .assert()
        .success()
        .stdout(indoc! { "
            * /b
        "});
}
