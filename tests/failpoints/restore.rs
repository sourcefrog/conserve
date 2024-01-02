// Copyright 2024 Martin Pool

//! Simulate IO errors during restore.

use std::path::Path;

use assert_fs::TempDir;
use conserve::monitor::collect::CollectMonitor;
use conserve::transport::open_local_transport;
use fail::FailScenario;

use conserve::*;

#[test]
fn create_dir_permission_denied() {
    let scenario = FailScenario::setup();
    fail::cfg("conserve::restore::create-dir", "return").unwrap();
    let archive =
        Archive::open(open_local_transport(Path::new("testdata/archive/simple/v0.6.10")).unwrap())
            .unwrap();
    let options = RestoreOptions {
        ..RestoreOptions::default()
    };
    let restore_tmp = TempDir::new().unwrap();
    let monitor = CollectMonitor::arc();
    let stats = restore(&archive, restore_tmp.path(), &options, monitor.clone()).expect("Restore");
    dbg!(&stats);
    dbg!(&monitor.problems.lock().unwrap());
    assert_eq!(stats.errors, 3);
    // TODO: Check that the monitor saw the errors too, once that's hooked up.
    scenario.teardown();
}
