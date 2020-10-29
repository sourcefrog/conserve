// Copyright 2017 Julian Raufelder.
// Copyright 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Create GlobSet from a list of strings

use globset::{Glob, GlobSet, GlobSetBuilder};

use super::*;

pub fn from_strings<I: IntoIterator<Item = S>, S: AsRef<str>>(
    excludes: I,
) -> Result<Option<GlobSet>> {
    let mut builder = GlobSetBuilder::new();
    let mut empty = true;
    for i in excludes {
        builder.add(Glob::new(i.as_ref()).map_err(|source| Error::ParseGlob { source })?);
        empty = false;
    }
    if empty {
        return Ok(None);
    }
    builder.build().map_err(Into::into).map(Some)
}

pub fn excludes_nothing() -> GlobSet {
    GlobSetBuilder::new().build().unwrap()
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    pub fn simple_parse() {
        let vec = vec!["fo*", "foo", "bar*"];
        let excludes = excludes::from_strings(&vec).expect("ok").expect("some");
        assert_eq!(excludes.matches("foo").len(), 2);
        assert_eq!(excludes.matches("foobar").len(), 1);
        assert_eq!(excludes.matches("barBaz").len(), 1);
        assert_eq!(excludes.matches("bazBar").len(), 0);
    }

    #[test]
    pub fn path_parse() {
        let excludes = excludes::from_strings(&["fo*/bar/baz*"])
            .expect("ok")
            .expect("some");
        assert_eq!(excludes.matches("foo/bar/baz.rs").len(), 1);
    }

    #[test]
    pub fn extendend_pattern_parse() {
        let excludes = excludes::from_strings(&["fo?", "ba[abc]", "[!a-z]"])
            .expect("ok")
            .expect("some");
        assert_eq!(excludes.matches("foo").len(), 1);
        assert_eq!(excludes.matches("fo").len(), 0);
        assert_eq!(excludes.matches("baa").len(), 1);
        assert_eq!(excludes.matches("1").len(), 1);
        assert_eq!(excludes.matches("a").len(), 0);
    }

    #[test]
    pub fn nothing_parse() {
        let excludes = excludes::excludes_nothing();
        assert!(excludes.matches("a").is_empty());
    }
}
