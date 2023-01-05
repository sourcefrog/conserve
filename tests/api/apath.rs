// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2021 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

use conserve::Apath;

#[test]
fn parse() {
    use conserve::apath::Apath;
    let apath: Apath = "/something".parse().unwrap();
    assert_eq!(apath.to_string(), "/something");
}

#[test]
fn is_prefix_of() {
    use std::ops::Not;
    assert!(Apath::from("/").is_prefix_of(&Apath::from("/stuff")));
    assert!(Apath::from("/").is_prefix_of(&Apath::from("/")));
    assert!(Apath::from("/stuff").is_prefix_of(&Apath::from("/stuff/file")));
    assert!(Apath::from("/stuff/file")
        .is_prefix_of(&Apath::from("/stuff"))
        .not());
    assert!(Apath::from("/this")
        .is_prefix_of(&Apath::from("/that"))
        .not());
    assert!(Apath::from("/this")
        .is_prefix_of(&Apath::from("/that/other"))
        .not());
}

#[test]
pub fn invalid() {
    let invalid_cases = [
        "",
        "//",
        "//a",
        "/a//b",
        "/a/",
        "/a//",
        "./a/b",
        "/./a/b",
        "/a/b/.",
        "/a/./b",
        "/a/b/../c",
        "../a",
        "/hello\0",
    ];
    for v in invalid_cases.iter() {
        assert!(!Apath::is_valid(v), "{v:?} incorrectly marked valid");
    }
}

#[test]
pub fn valid_and_ordered() {
    let ordered = [
        "/",
        "/...a",
        "/.a",
        "/a",
        "/b",
        "/kleine Katze Fuß",
        "/~~",
        "/ñ",
        "/a/...",
        "/a/..obscure",
        "/a/.config",
        "/a/1",
        "/a/100",
        "/a/2",
        "/a/añejo",
        "/a/b/c",
        "/b/((",
        "/b/,",
        "/b/A",
        "/b/AAAA",
        "/b/a",
        "/b/b",
        "/b/c",
        "/b/a/c",
        "/b/b/c",
        "/b/b/b/z",
        "/b/b/b/{zz}",
    ];
    for (i, a) in ordered.iter().enumerate() {
        assert!(Apath::is_valid(a), "{a:?} incorrectly marked invalid");
        let ap = Apath::from(*a);
        // Check it can be formatted
        assert_eq!(format!("{ap}"), *a);
        for (j, b) in ordered.iter().enumerate() {
            let expected_order = i.cmp(&j);
            let bp = Apath::from(*b);
            let r = ap.cmp(&bp);
            assert_eq!(
                r, expected_order,
                "cmp({ap:?}, {bp:?}): returned {r:?} expected {expected_order:?}"
            );
        }
    }
}
