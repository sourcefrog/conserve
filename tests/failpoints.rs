// Copyright 2024 Martin Pool

//! Tests based on failpoints, simulating IO errors, or other hard-to-reproduce
//! conditions.
//!
//! The tests in this directory simulate IO errors or other failures in Conserve, and assert that Conserve handles and reports them correctly.
//!
//! Failpoints aren't built by default.
//!
//! To run them use
//!
//!     cargo test --features fail/failpoints --test failpoints
//!

use std::io;
use std::path::Path;

use assert_fs::TempDir;
use conserve::monitor::test::TestMonitor;
use fail::FailScenario;

use conserve::*;
use transport::Transport2;

#[test]
fn create_dir_permission_denied() {
    let scenario = FailScenario::setup();
    fail::cfg("restore::create-dir", "return").unwrap();
    let archive = Archive::open(Transport2::local(Path::new(
        "testdata/archive/simple/v0.6.10",
    )))
    .unwrap();
    let options = RestoreOptions {
        ..RestoreOptions::default()
    };
    let restore_tmp = TempDir::new().unwrap();
    let monitor = TestMonitor::arc();
    let stats = restore(&archive, restore_tmp.path(), &options, monitor.clone()).expect("Restore");
    dbg!(&stats);
    let errors = monitor.take_errors();
    dbg!(&errors);
    assert_eq!(errors.len(), 2);
    if let Error::RestoreDirectory { path, .. } = &errors[0] {
        assert!(path.ends_with("subdir"));
    } else {
        panic!("Unexpected error {:?}", errors[0]);
    }
    // Also, since we didn't create the directory, we fail to create the file within it.
    if let Error::RestoreFile { path, source } = &errors[1] {
        assert!(path.ends_with("subdir/subfile"));
        assert_eq!(source.kind(), io::ErrorKind::NotFound);
    } else {
        panic!("Unexpected error {:?}", errors[1]);
    }
    scenario.teardown();
}
