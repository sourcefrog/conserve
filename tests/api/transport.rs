// Copyright 2021, 2022 Martin Pool.

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
use url::Url;

use conserve::transport::{open_transport, ListDir};

#[test]
fn open_local() {
    open_transport("/backup").unwrap();
}

#[test]
fn list_dir_names() {
    let temp = assert_fs::TempDir::new().unwrap();
    temp.child("a dir").create_dir_all().unwrap();
    temp.child("a file").touch().unwrap();
    temp.child("another file").touch().unwrap();

    let url = Url::from_directory_path(temp.path()).unwrap();
    dbg!(&url);
    let transport = open_transport(url.as_str()).unwrap();
    dbg!(&transport);

    let ListDir { mut files, dirs } = transport.list_dir("").unwrap();
    assert_eq!(dirs, ["a dir"]);
    files.sort();
    assert_eq!(files, ["a file", "another file"]);

    temp.close().unwrap();
}

#[test]
fn parse_location_urls() {
    for n in [
        "./relative",
        "/backup/repo.c6",
        "../backup/repo.c6",
        "c:/backup/repo",
        r"c:\backup\repo\",
    ] {
        assert!(open_transport(n).is_ok(), "Failed to parse {n:?}");
    }
}

#[test]
fn unsupported_location_urls() {
    assert_eq!(
        open_transport("http://conserve.example/repo")
            .unwrap_err()
            .to_string(),
        "Unsupported URL scheme \"http\""
    );
    assert_eq!(
        open_transport("ftp://user@conserve.example/repo")
            .unwrap_err()
            .to_string(),
        "Unsupported URL scheme \"ftp\""
    );
}
