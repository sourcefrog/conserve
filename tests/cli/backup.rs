// Conserve backup system.
// Copyright 2016, 2017, 2018, 2019, 2020, 2021, 2022 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use assert_cmd::prelude::*;

use conserve::test_fixtures::{ScratchArchive, TreeFixture};

use crate::run_conserve;

#[test]
fn backup_verbose() {
    let af = ScratchArchive::new();
    let src = TreeFixture::new();
    src.create_dir("subdir");
    src.create_file("subdir/a");
    src.create_file("subdir/b");

    run_conserve()
        .args(["backup", "--no-stats", "-v", "-R"])
        .arg(af.path())
        .arg(src.path())
        .assert()
        .success()
        .stdout("+ /subdir/a\n+ /subdir/b\n");
}
