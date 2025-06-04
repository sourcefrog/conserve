// Copyright 2021-2023 Martin Pool.

//! Utility functions shared by tests.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// Make a copy of a archive testdata.
pub fn copy_testdata_archive(name: &str, version: &str) -> TempDir {
    let temp = TempDir::with_prefix(format!("conserve-api-test-{name}-{version}"))
        .expect("create temp dir");
    let stored_archive_path = testdata_archive_path(name, version);
    cp_r::CopyOptions::default()
        .copy_tree(stored_archive_path, temp.path())
        .expect("copy archive tree");
    temp
}

pub fn testdata_archive_path(name: &str, version: &str) -> PathBuf {
    Path::new("testdata/archive")
        .join(name)
        .join(format!("v{version}/"))
}
