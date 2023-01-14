// Conserve backup system.
// Copyright 2020-2023 Martin Pool.

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

use assert_matches::assert_matches;

use conserve::monitor::CollectValidateMonitor;
use conserve::*;

#[test]
fn missing_block() -> Result<()> {
    let archive = Archive::open_path(Path::new("testdata/damaged/missing-block"))?;
    let mut monitor = CollectValidateMonitor::new();
    let _validate_stats = archive.validate(&ValidateOptions::default(), &mut monitor)?;
    assert_matches!(
        monitor.into_problems().as_slice(),
        [Error::BlockMissing { .. }]
    );
    Ok(())
}

#[test]
fn missing_block_skip_block_hashes() -> Result<()> {
    let archive = Archive::open_path(Path::new("testdata/damaged/missing-block"))?;
    let mut monitor = CollectValidateMonitor::new();
    let _validate_stats = archive.validate(
        &ValidateOptions {
            skip_block_hashes: true,
        },
        &mut monitor,
    )?;
    assert_matches!(
        monitor.into_problems().as_slice(),
        [Error::BlockMissing { .. }]
    );
    Ok(())
}
