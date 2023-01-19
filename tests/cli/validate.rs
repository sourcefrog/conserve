//! Tests for the `conserve validate` CLI.

use assert_cmd::prelude::*;
use assert_fs::prelude::*;
use assert_fs::TempDir;
use predicates::prelude::*;

use super::run_conserve;

/// <https://github.com/sourcefrog/conserve/issues/171>
#[test]
fn validate_does_not_complain_about_gc_lock() {
    let temp = TempDir::new().unwrap();
    run_conserve()
        .args(["init"])
        .arg(temp.path())
        .assert()
        .success();
    temp.child("GC_LOCK").touch().unwrap();
    run_conserve()
        .args(["validate"])
        .arg(temp.path())
        .assert()
        .stdout(predicate::str::contains("Unexpected file").not())
        .success();
}

#[test]
fn validate_non_fatal_problems_nonzero_result() {
    run_conserve()
        .args(["validate", "testdata/damaged/missing-block/"])
        .assert()
        .stderr(predicate::str::contains("Archive has some problems."))
        .code(2);
}
