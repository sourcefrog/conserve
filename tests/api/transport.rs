// Copyright 2021 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use assert_fs::prelude::*;

use conserve::transport::{ListDirNames, Location};

#[test]
fn open_local() {
    let location = Location::Local("/backup".to_owned().into());
    let _transport = location.open().unwrap();
}

#[test]
fn parse_location() {
    use conserve::transport::Location;
    use std::str::FromStr;
    let location: Location = Location::from_str("/backup/example").unwrap();
    let _transport = location.open();
}

#[test]
fn list_dir_names() {
    let temp = assert_fs::TempDir::new().unwrap();
    temp.child("a dir").create_dir_all().unwrap();
    temp.child("a file").touch().unwrap();
    temp.child("another file").touch().unwrap();

    let transport = Location::Local(temp.path().to_path_buf()).open().unwrap();

    let ListDirNames { mut files, dirs } = transport.list_dir_names("").unwrap();
    assert_eq!(dirs, ["a dir"]);
    files.sort();
    assert_eq!(files, ["a file", "another file"]);

    temp.close().unwrap();
}
