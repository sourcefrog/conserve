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

use conserve::monitor::CollectMonitor;
use conserve::*;

#[test]
fn missing_block() -> Result<()> {
    let archive = Archive::open_path(Path::new("testdata/damaged/missing-block"))?;
    let mut monitor = CollectMonitor::new();
    archive.validate(&ValidateOptions::default(), &mut monitor)?;
    assert_eq!(monitor.error_messages(), &["Block fec91c70284c72d0d4e3684788a90de9338a5b2f47f01fedbe203cafd68708718ae5672d10eca804a8121904047d40d1d6cf11e7a76419357a9469af41f22d01 is missing"]);
    Ok(())
}

#[test]
fn missing_block_skip_block_hashes() -> Result<()> {
    let archive = Archive::open_path(Path::new("testdata/damaged/missing-block"))?;
    let mut monitor = CollectMonitor::new();
    archive.validate(
        &ValidateOptions {
            skip_block_hashes: true,
        },
        &mut monitor,
    )?;
    assert_eq!(monitor.error_messages(), ["Block fec91c70284c72d0d4e3684788a90de9338a5b2f47f01fedbe203cafd68708718ae5672d10eca804a8121904047d40d1d6cf11e7a76419357a9469af41f22d01 is missing"]);
    Ok(())
}
