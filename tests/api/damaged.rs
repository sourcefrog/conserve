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
use std::sync::Arc;

use conserve::monitor::test::TestMonitor;
use tracing_test::traced_test;

use conserve::*;

#[traced_test]
#[test]
fn missing_block_when_checking_hashes() -> Result<()> {
    let archive = Archive::open_path(Path::new("testdata/damaged/missing-block"))?;
    archive.validate(&ValidateOptions::default(), Arc::new(TestMonitor::new()))?;
    assert!(logs_contain(
        "Referenced block missing block_hash=fec91c70284c72d0d4e3684788a90de9338a5b2f47f01fedbe203cafd68708718ae5672d10eca804a8121904047d40d1d6cf11e7a76419357a9469af41f22d01"));
    Ok(())
}

#[traced_test]
#[test]
fn missing_block_skip_block_hashes() -> Result<()> {
    let archive = Archive::open_path(Path::new("testdata/damaged/missing-block"))?;
    archive.validate(
        &ValidateOptions {
            skip_block_hashes: true,
        },
        Arc::new(TestMonitor::new()),
    )?;
    assert!(logs_contain(
        "Referenced block missing block_hash=fec91c70284c72d0d4e3684788a90de9338a5b2f47f01fedbe203cafd68708718ae5672d10eca804a8121904047d40d1d6cf11e7a76419357a9469af41f22d01"));
    Ok(())
}
