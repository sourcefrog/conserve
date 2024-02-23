// Copyright 2023 Martin Pool

//! Tests for the `conserve validate` CLI.

use std::path::Path;

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use assert_fs::{NamedTempFile, TempDir};
use predicates::prelude::*;
use serde_json::json;
use serde_json::{Deserializer, Value};
use tracing::Level;

use crate::run_conserve;

fn read_log_json(path: &Path) -> Vec<serde_json::Value> {
    let json_content = std::fs::read_to_string(path).unwrap();
    println!("{json_content}");
    Deserializer::from_str(&json_content)
        .into_iter::<Value>()
        .map(Result::unwrap)
        .collect::<Vec<Value>>()
}

/// Filter out only logs with severity equal or more important than `level`.
fn filter_by_level(logs: &[serde_json::Value], level: Level) -> Vec<&serde_json::Value> {
    logs.iter()
        .filter(move |event| event["level"].as_str().unwrap().parse::<Level>().unwrap() <= level)
        .collect()
}

// /// Reduce json logs to just their messages.
// fn events_to_messages<'s, I>(logs: I) -> Vec<&'s str>
// where
//     I: IntoIterator<Item = &'s serde_json::Value>,
// {
//     logs.into_iter()
//         .map(|event| event["fields"]["message"].as_str().unwrap())
//         .collect()
// }

/// <https://github.com/sourcefrog/conserve/issues/171>
#[test]
fn validate_does_not_complain_about_gc_lock() {
    let temp = TempDir::new().unwrap();
    let log_temp = NamedTempFile::new("log.json").unwrap();
    run_conserve()
        .args(["init"])
        .arg(temp.path())
        .assert()
        .success();
    temp.child("GC_LOCK").touch().unwrap();
    run_conserve()
        .args(["validate"])
        .arg("--log-json")
        .arg(log_temp.path())
        .arg(temp.path())
        .assert()
        .stdout(predicate::str::contains("Unexpected file").not())
        .success();
    let events = read_log_json(log_temp.path());
    dbg!(&events);
    assert!(filter_by_level(&events, Level::WARN).is_empty());
}

#[test]
fn validate_non_fatal_problems_nonzero_result_and_json_log() {
    let log_temp = NamedTempFile::new("log.json").unwrap();
    run_conserve()
        .args(["validate", "testdata/damaged/missing-block/"])
        .arg("--log-json")
        .arg(log_temp.path())
        .assert()
        .stderr(predicate::str::contains("Archive has some problems."))
        .code(2);
    let events = read_log_json(log_temp.path());
    dbg!(&events);
    let errors = filter_by_level(&events, Level::ERROR);
    // TODO: Write errors to json, read that json too.
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0]["fields"],
        json!({
            "message": "Referenced block fec91c70284c72d0d4e3684788a90de9338a5b2f47f01fedbe203cafd68708718ae5672d10eca804a8121904047d40d1d6cf11e7a76419357a9469af41f22d01 is missing",
        })
    );
}
