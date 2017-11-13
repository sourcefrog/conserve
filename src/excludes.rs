// Copyright 2017 Julian Raufelder.

//! Create GlobSet from a list of strings

use globset::{Glob, GlobSet, GlobSetBuilder};

use super::*;

pub fn parse_excludes(excludes: Option<Vec<&str>>) -> Result<Option<GlobSet>> {
    match excludes {
        Some(excludes) => {
            let mut builder = GlobSetBuilder::new();
            for exclude in excludes {
                builder.add(Glob::new(exclude)
                    .chain_err(|| format!("Failed to parse exclude value: {}", exclude))?);
            }
            Ok(Some(builder.build()
                .chain_err(|| "Failed to build exclude patterns")?))
        }
        None => Ok(None)
    }
}


#[cfg(test)]
mod tests {
    use spectral::prelude::*;
    use super::super::*;


    #[test]
    pub fn simple_parse() {
        let vec = vec!["fo*", "foo", "bar*"];
        let excludes = excludes::parse_excludes(Some(vec)).unwrap();
        assert_that(&excludes.clone().unwrap().matches("foo").len()).is_equal_to(2);
        assert_that(&excludes.clone().unwrap().matches("foobar").len()).is_equal_to(1);
        assert_that(&excludes.clone().unwrap().matches("barBaz").len()).is_equal_to(1);
        assert_that(&excludes.clone().unwrap().matches("bazBar").len()).is_equal_to(0);
    }

    #[test]
    pub fn path_parse() {
        let vec = vec!["fo*/bar/baz*"];
        let excludes = excludes::parse_excludes(Some(vec)).unwrap();
        assert_that(&excludes.clone().unwrap().matches("foo/bar/baz.rs").len()).is_equal_to(1);
    }

    #[test]
    pub fn extendend_pattern_parse() {
        let vec = vec!["fo?", "ba[abc]", "[!a-z]"];
        let excludes = excludes::parse_excludes(Some(vec)).unwrap();
        assert_that(&excludes.clone().unwrap().matches("foo").len()).is_equal_to(1);
        assert_that(&excludes.clone().unwrap().matches("fo").len()).is_equal_to(0);
        assert_that(&excludes.clone().unwrap().matches("baa").len()).is_equal_to(1);
        assert_that(&excludes.clone().unwrap().matches("1").len()).is_equal_to(1);
        assert_that(&excludes.clone().unwrap().matches("a").len()).is_equal_to(0);
    }

    #[test]
    pub fn nothing_parse() {
        let excludes = excludes::parse_excludes(None).unwrap();
        assert_that(&excludes).is_none()
    }
}