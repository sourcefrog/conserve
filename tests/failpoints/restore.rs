// Copyright 2024 Martin Pool

//! Simulate IO errors during restore.

use std::io;
use std::path::Path;

use assert_fs::TempDir;
use conserve::monitor::test::TestMonitor;
use conserve::transport::open_local_transport;
use fail::FailScenario;

use conserve::*;

#[test]
fn create_dir_permission_denied() {
    let scenario = FailScenario::setup();
    fail::cfg("restore::create-dir", "return").unwrap();
    let archive =
        Archive::open(open_local_transport(Path::new("testdata/archive/simple/v0.6.10")).unwrap())
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
