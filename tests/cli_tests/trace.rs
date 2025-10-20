// Copyright 2023 Martin Pool

//! Tests for trace-related options and behaviors of the Conserve CLI.

use assert_cmd::prelude::*;
use assert_fs::TempDir;
use assert_fs::prelude::*;
use predicates::prelude::*;

use crate::run_conserve;

#[test]
fn no_trace_timestamps_by_default() {
    let temp_dir = TempDir::new().unwrap();
    run_conserve()
        .args(["-D", "init"])
        .arg(temp_dir.child("archive").path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "TRACE conserve::termui::trace: Tracing enabled",
        ));
}
