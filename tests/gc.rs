// Copyright 2015, 2016, 2017, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Test Conserve through its public API.

use conserve::test_fixtures::{ScratchArchive, TreeFixture};
use conserve::*;

#[test]
fn unreferenced_blocks() {
    let af = ScratchArchive::new();
    let tf = TreeFixture::new();
    tf.create_file("hello");
    let content_hash: BlockHash =
        "9063990e5c5b2184877f92adace7c801a549b00c39cd7549877f06d5dd0d3a6ca6eee42d5\
        896bdac64831c8114c55cee664078bd105dc691270c92644ccb2ce7"
            .parse()
            .unwrap();

    let _copy_stats = af
        .backup(&tf.path(), &BackupOptions::default())
        .expect("backup");

    // Delete the band and index
    std::fs::remove_dir_all(af.path().join("b0000")).unwrap();

    let unreferenced: Vec<BlockHash> = af.unreferenced_blocks().unwrap().collect();
    assert_eq!(unreferenced, [content_hash]);
}
