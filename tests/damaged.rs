// Conserve backup system.
// Copyright 2020-2024 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Test validation of archives with some problems.

use std::path::Path;

use conserve::monitor::test::TestMonitor;
use tracing_test::traced_test;

use conserve::*;

#[traced_test]
#[tokio::test]
async fn missing_block_when_checking_hashes() -> Result<()> {
    let archive = Archive::open_path(Path::new("testdata/damaged/missing-block")).await?;
    let monitor = TestMonitor::arc();
    archive
        .validate(&ValidateOptions::default(), monitor.clone())
        .await
        .unwrap();
    let errors = monitor.take_errors();
    dbg!(&errors);
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], Error::BlockMissing { .. }));
    Ok(())
}

#[traced_test]
#[tokio::test]
async fn missing_block_skip_block_hashes() -> Result<()> {
    let archive = Archive::open_path(Path::new("testdata/damaged/missing-block")).await?;
    let monitor = TestMonitor::arc();
    archive
        .validate(
            &ValidateOptions {
                skip_block_hashes: true,
            },
            monitor.clone(),
        )
        .await?;
    let errors = monitor.take_errors();
    dbg!(&errors);
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0], Error::BlockMissing { .. }));
    Ok(())
}
