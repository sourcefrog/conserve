// Conserve backup system.
// Copyright 2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Test `conserve ls`.

use assert_cmd::prelude::*;
use indoc::indoc;
use pretty_assertions::assert_eq;

use super::run_conserve;

#[test]
fn ls_json() {
    let cmd = run_conserve()
        .args(["ls", "--json", "./testdata/archive/minimal/v0.6.17"])
        .assert()
        .success();
    assert_eq!(
        String::from_utf8_lossy(&cmd.get_output().stdout),
        indoc! { r#"
            {"apath":"/","kind":"Dir","mtime":"2020-06-16 00:15:23.0 +00:00:00","unix_mode":509,"user":"mbp","group":"mbp"}
            {"apath":"/hello","kind":"File","size":12,"mtime":"2020-06-16 00:15:23.0 +00:00:00","unix_mode":436,"user":"mbp","group":"mbp"}
            {"apath":"/subdir","kind":"Dir","mtime":"2020-06-16 00:15:23.0 +00:00:00","unix_mode":509,"user":"mbp","group":"mbp"}
            {"apath":"/subdir/subfile","kind":"File","size":12,"mtime":"2020-06-16 00:15:23.0 +00:00:00","unix_mode":436,"user":"mbp","group":"mbp"}
        "# }
    );
}
