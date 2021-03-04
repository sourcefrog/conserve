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

//! Test deletion.

use conserve::test_fixtures::ScratchArchive;
use conserve::*;

#[test]
fn delete_all_bands() {
    let af = ScratchArchive::new();
    af.store_two_versions();

    let stats = af
        .delete_bands(&[BandId::new(&[0]), BandId::new(&[1])], &Default::default())
        .expect("delete_bands");

    assert_eq!(stats.deleted_block_count, 2);
    assert_eq!(stats.deleted_band_count, 2);
}
