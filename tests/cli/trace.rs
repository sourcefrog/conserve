// Copyright 2023 Martin Pool

//! Tests for trace-related options and behaviors of the Conserve CLI.

use assert_fs::prelude::*;
use predicates::prelude::*;

use super::*;

#[test]
fn no_trace_timestamps_by_default() {
    let temp_dir = TempDir::new().unwrap();
    // Maybe we should disable coloring if the destination is not a
    // tty, but for now they are there...
    run_conserve()
        .args(["-D", "init"])
        .arg(temp_dir.child("archive").path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "TRACE conserve::ui: Tracing enabled",
        ));
}
