// Copyright 2017 Julian Raufelder.
// Copyright 2020-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Exclude files from operations based on globs, etc.
//!
//! Globs match against Apaths.
//!
//! Patterns that start with a slash match only against full paths from the top
//! of the tree. Patterns that do not start with a slash match the suffix of the
//! path.

use std::borrow::Cow;
use std::fs;
use std::iter::empty;
use std::path::Path;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use super::*;

/// Describes which files to exclude from a backup, restore, etc.
#[derive(Clone, Debug)]
pub struct Exclude {
    globset: GlobSet,
    // TODO: Control of matching cachedir.
}

impl Exclude {
    /// Create an [Exclude] from a list of glob strings.
    ///
    /// The globs match against the apath, which will
    /// always start with a `/`.
    ///
    /// Globs are extended to also match any children of matching paths.
    pub fn from_strings<I: IntoIterator<Item = S>, S: AsRef<str>>(excludes: I) -> Result<Exclude> {
        Exclude::from_patterns_and_files(excludes, empty::<&Path>())
    }

    /// Build from a list of exclusion patterns and a list of files containing more patterns.
    pub fn from_patterns_and_files<I1, A, I2, P>(exclude: I1, exclude_from: I2) -> Result<Exclude>
    where
        I1: IntoIterator<Item = A>,
        A: AsRef<str>,
        I2: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut gsb = GlobSetBuilder::new();
        for pat in exclude {
            add_pattern(&mut gsb, pat.as_ref())?;
        }
        for path in exclude_from {
            add_patterns_from_file(&mut gsb, path.as_ref())?;
        }
        Ok(Exclude {
            globset: gsb.build()?,
        })
    }

    /// Exclude nothing, even items that might be excluded by default.
    pub fn nothing() -> Exclude {
        Exclude {
            globset: GlobSet::empty(),
        }
    }

    /// True if this apath should be excluded.
    pub fn matches<'a, A>(&self, apath: &'a A) -> bool
    where
        &'a A: Into<Apath> + 'a,
        A: ?Sized,
    {
        let apath: Apath = apath.into();
        self.globset.is_match(apath)
    }
}

/// Add one pattern with Conserve's semantics.
fn add_pattern(gsb: &mut GlobSetBuilder, pattern: &str) -> Result<()> {
    let pattern: Cow<str> = if pattern.starts_with('/') {
        Cow::Borrowed(pattern)
    } else {
        Cow::Owned(format!("**/{pattern}"))
    };
    gsb.add(
        GlobBuilder::new(&pattern)
            .literal_separator(true)
            .build()
            .map_err(|source| Error::ParseGlob { source })?,
    );
    gsb.add(
        GlobBuilder::new(&format!("{pattern}/**"))
            .literal_separator(true)
            .build()
            .map_err(|source| Error::ParseGlob { source })?,
    );
    Ok(())
}

fn add_patterns_from_file(gsb: &mut GlobSetBuilder, path: &Path) -> Result<()> {
    for pat in fs::read_to_string(path)?
        .lines()
        .map(str::trim)
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        add_pattern(gsb, pat)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn simple_globs() {
        let vec = vec!["foo*", "quo", "bar*"];
        let exclude = Exclude::from_strings(vec).unwrap();

        // Matches in the root
        assert!(exclude.matches("/quo"));
        assert!(exclude.matches("/foo"));
        assert!(exclude.matches("/foobar"));
        assert!(exclude.matches("/barBaz"));
        assert!(!exclude.matches("/bazBar"));

        // Also matches in a subdir
        assert!(exclude.matches("/subdir/foo"));
        assert!(exclude.matches("/subdir/foobar"));
        assert!(exclude.matches("/subdir/barBaz"));
        assert!(!exclude.matches("/subdir/bazBar"));
    }

    #[test]
    fn rooted_pattern() {
        let exclude = Exclude::from_strings(["/exc"]).unwrap();

        assert!(exclude.matches("/exc"));
        assert!(!exclude.matches("/excellent"));
        assert!(!exclude.matches("/sub/excellent"));
        assert!(!exclude.matches("/sub/exc"));
    }

    #[test]
    fn path_parse() {
        let exclude = Exclude::from_strings(["foo*/bar/baz*"]).unwrap();
        assert!(exclude.matches("/foo1/bar/baz.rs"))
    }

    #[test]
    fn extended_pattern_parse() {
        // Note that these are globs, not regexps, so "foo?" means "foo" followed by one character.
        let exclude = Exclude::from_strings(["foo?", "bar[abc]", "[!a-z]"]).unwrap();
        assert!(exclude.matches("/foox"));
        assert!(!exclude.matches("/foo"));
        assert!(!exclude.matches("/bar"));
        assert!(exclude.matches("/bara"));
        assert!(exclude.matches("/barb"));
        assert!(exclude.matches("/1"));
        assert!(!exclude.matches("/a"));
    }

    #[test]
    fn nothing_parse() {
        let exclude = Exclude::nothing();
        assert!(!exclude.matches("/a"));
    }
}
