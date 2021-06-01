// Copyright 2017 Julian Raufelder.
// Copyright 2020, 2021 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Create a [GlobSet] from a list of strings.
//!
//! The [GlobSet] is intended to be matched against an [Apath].
//!
//! Patterns that start with a slash match only against full paths from the top
//! of the tree. Patterns that do not start with a slash match the suffix of the
//! path.

use std::borrow::Cow;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use super::*;

pub struct ExcludeBuilder {
    gsb: GlobSetBuilder,
}

impl ExcludeBuilder {
    pub fn new() -> ExcludeBuilder {
        ExcludeBuilder {
            gsb: GlobSetBuilder::new(),
        }
    }

    pub fn build(&self) -> Result<GlobSet> {
        Ok(self.gsb.build()?)
    }

    pub fn add(&mut self, pat: &str) -> Result<&mut ExcludeBuilder> {
        let pat: Cow<str> = if pat.starts_with('/') {
            Cow::Borrowed(pat)
        } else {
            Cow::Owned(format!("**/{}", pat))
        };
        let glob = GlobBuilder::new(&pat)
            .literal_separator(true)
            .build()
            .map_err(|source| Error::ParseGlob { source })?;
        self.gsb.add(glob);
        Ok(self)
    }

    pub fn add_file(&mut self, path: &Path) -> Result<&mut ExcludeBuilder> {
        self.add_from_read(&mut File::open(path)?)
    }

    /// Create a [GlobSet] from lines in a file, with one pattern per line.
    ///
    /// Lines starting with `#` are comments, and leading and trailing whitespace is removed.
    pub fn add_from_read(&mut self, f: &mut dyn Read) -> Result<&mut ExcludeBuilder> {
        let mut b = String::new();
        f.read_to_string(&mut b)?;
        for pat in b
            .lines()
            .map(str::trim)
            .filter(|s| !s.starts_with('#') && !s.is_empty())
        {
            self.add(pat)?;
        }
        Ok(self)
    }

    /// Build from command line arguments of patterns and filenames.
    pub fn from_args(exclude: &[String], exclude_from: &[String]) -> Result<ExcludeBuilder> {
        let mut builder = ExcludeBuilder::new();
        for pat in exclude {
            builder.add(pat)?;
        }
        for path in exclude_from {
            builder.add_file(Path::new(path))?;
        }
        Ok(builder)
    }
}

impl Default for ExcludeBuilder {
    fn default() -> Self {
        ExcludeBuilder::new()
    }
}

/// Create a [GlobSet] from a list of strings.
///
/// The [GlobSet] is intended to be matched against an [Apath], which will
/// always start with a `/`.
pub fn from_strings<I: IntoIterator<Item = S>, S: AsRef<str>>(excludes: I) -> Result<GlobSet> {
    let mut builder = ExcludeBuilder::new();
    for s in excludes {
        builder.add(s.as_ref())?;
    }
    builder.build()
}

pub fn excludes_nothing() -> GlobSet {
    GlobSet::empty()
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn simple_globs() {
        let vec = vec!["fo*", "foo", "bar*"];
        let excludes = excludes::from_strings(&vec).expect("ok");

        // Matches in the root
        assert_eq!(excludes.matches("/foo").len(), 2);
        assert_eq!(excludes.matches("/foobar").len(), 1);
        assert_eq!(excludes.matches("/barBaz").len(), 1);
        assert_eq!(excludes.matches("/bazBar").len(), 0);

        // Also matches in a subdir
        assert!(excludes.is_match("/subdir/foo"));
        assert!(excludes.is_match("/subdir/foobar"));
        assert!(excludes.is_match("/subdir/barBaz"));
        assert!(!excludes.is_match("/subdir/bazBar"));
    }

    #[test]
    fn rooted_pattern() {
        let excludes = excludes::from_strings(&["/exc"]).unwrap();

        assert!(excludes.is_match("/exc"));
        assert!(!excludes.is_match("/excellent"));
        assert!(!excludes.is_match("/sub/excellent"));
        assert!(!excludes.is_match("/sub/exc"));
    }

    #[test]
    fn path_parse() {
        let excludes = excludes::from_strings(&["fo*/bar/baz*"]).unwrap();
        assert_eq!(excludes.matches("foo/bar/baz.rs").len(), 1);
    }

    #[test]
    fn extendend_pattern_parse() {
        let excludes = excludes::from_strings(&["fo?", "ba[abc]", "[!a-z]"]).unwrap();
        assert_eq!(excludes.matches("foo").len(), 1);
        assert_eq!(excludes.matches("fo").len(), 0);
        assert_eq!(excludes.matches("baa").len(), 1);
        assert_eq!(excludes.matches("1").len(), 1);
        assert_eq!(excludes.matches("a").len(), 0);
    }

    #[test]
    fn nothing_parse() {
        let excludes = excludes::excludes_nothing();
        assert!(excludes.matches("a").is_empty());
    }
}
