// Copyright 2023 Martin Pool

//! Tests for the `conserve validate` CLI.

use std::path::Path;

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use assert_fs::NamedTempFile;
use assert_fs::TempDir;
use predicates::prelude::*;
use serde_json::json;
use serde_json::Deserializer;
use serde_json::Value;

use super::run_conserve;

fn read_problems(problems_json_path: &Path) -> Vec<serde_json::Value> {
    let json_content = std::fs::read_to_string(&problems_json_path).unwrap();
    dbg!(&json_content);
    Deserializer::from_str(&json_content)
        .into_iter::<Value>()
        .map(Result::unwrap)
        .collect::<Vec<Value>>()
}

/// <https://github.com/sourcefrog/conserve/issues/171>
#[test]
fn validate_does_not_complain_about_gc_lock() {
    let temp = TempDir::new().unwrap();
    let problems_temp = NamedTempFile::new("problems.json").unwrap();
    run_conserve()
        .args(["init"])
        .arg(temp.path())
        .assert()
        .success();
    temp.child("GC_LOCK").touch().unwrap();
    run_conserve()
        .args(["validate", "--problems-json"])
        .arg(problems_temp.path())
        .arg(temp.path())
        .assert()
        .stdout(predicate::str::contains("Unexpected file").not())
        .success();
    let problems = read_problems(problems_temp.path());
    assert_eq!(&problems, &[] as &[serde_json::Value]);
}

#[test]
fn validate_non_fatal_problems_nonzero_result_and_json_problems() {
    let temp = TempDir::new().unwrap();
    let json_path = temp.path().join("problems.json");
    run_conserve()
        .args([
            "validate",
            "testdata/damaged/missing-block/",
            "--problems-json",
        ])
        .arg(&json_path)
        .assert()
        .stderr(predicate::str::contains("Archive has some problems."))
        .code(2);
    let problems = read_problems(&json_path);
    assert_eq!(
        &problems,
        &[json!({
            "BlockMissing": {
                "block_hash": "fec91c70284c72d0d4e3684788a90de9338a5b2f47f01fedbe203cafd68708718ae5672d10eca804a8121904047d40d1d6cf11e7a76419357a9469af41f22d01",
            }
        })]
    );
}
