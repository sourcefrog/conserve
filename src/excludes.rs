// Copyright 2017 Julian Raufelder.

//! Create GlobSet from a list of strings

use globset::{Glob, GlobSet, GlobSetBuilder};

use super::*;

pub fn from_strings(excludes: Vec<&str>) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();

    for exclude in excludes {
        builder.add(Glob::new(exclude).chain_err(|| {
            format!("Failed to parse exclude value: {}", exclude)
        })?);
    }

    Ok(builder.build().chain_err(
        || "Failed to build exclude patterns",
    )?)
}

pub fn excludes_nothing() -> GlobSet {
    GlobSetBuilder::new().build().unwrap()
}

#[cfg(test)]
mod tests {
    use spectral::prelude::*;
    use super::super::*;


    #[test]
    pub fn simple_parse() {
        let vec = vec!["fo*", "foo", "bar*"];
        let excludes = excludes::from_strings(vec).unwrap();
        assert_that(&excludes.matches("foo").len()).is_equal_to(2);
        assert_that(&excludes.matches("foobar").len()).is_equal_to(1);
        assert_that(&excludes.matches("barBaz").len()).is_equal_to(1);
        assert_that(&excludes.matches("bazBar").len()).is_equal_to(0);
    }

    #[test]
    pub fn path_parse() {
        let vec = vec!["fo*/bar/baz*"];
        let excludes = excludes::from_strings(vec).unwrap();
        assert_that(&excludes.matches("foo/bar/baz.rs").len()).is_equal_to(1);
    }

    #[test]
    pub fn extendend_pattern_parse() {
        let vec = vec!["fo?", "ba[abc]", "[!a-z]"];
        let excludes = excludes::from_strings(vec).unwrap();
        assert_that(&excludes.matches("foo").len()).is_equal_to(1);
        assert_that(&excludes.matches("fo").len()).is_equal_to(0);
        assert_that(&excludes.matches("baa").len()).is_equal_to(1);
        assert_that(&excludes.matches("1").len()).is_equal_to(1);
        assert_that(&excludes.matches("a").len()).is_equal_to(0);
    }

    #[test]
    pub fn nothing_parse() {
        let excludes = excludes::excludes_nothing();
        assert_that(&excludes.matches("a").len()).is_equal_to(0)
    }
}
