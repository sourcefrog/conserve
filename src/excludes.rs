// Copyright 2017 Julian Raufelder.
// Copyright 2020, 2021, 2022 Martin Pool.

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
use std::fs::File;
use std::io::Read;
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
    pub fn from_strings<I: IntoIterator<Item = S>, S: AsRef<str>>(excludes: I) -> Result<Exclude> {
        let mut builder = ExcludeBuilder::new();
        for s in excludes {
            builder.add(s.as_ref())?;
        }
        builder.build()
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

/// Construct Exclude object.
pub struct ExcludeBuilder {
    gsb: GlobSetBuilder,
}

impl ExcludeBuilder {
    pub fn new() -> ExcludeBuilder {
        ExcludeBuilder {
            gsb: GlobSetBuilder::new(),
        }
    }

    pub fn build(&self) -> Result<Exclude> {
        Ok(Exclude {
            globset: self.gsb.build()?,
        })
    }

    pub fn add(&mut self, pat: &str) -> Result<&mut ExcludeBuilder> {
        let pat: Cow<str> = if pat.starts_with('/') {
            Cow::Borrowed(pat)
        } else {
            Cow::Owned(format!("**/{pat}"))
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

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn simple_globs() {
        let vec = vec!["fo*", "foo", "bar*"];
        let exclude = Exclude::from_strings(vec).unwrap();

        // Matches in the root
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
        let exclude = Exclude::from_strings(["fo*/bar/baz*"]).unwrap();
        assert!(exclude.matches("/foo/bar/baz.rs"))
    }

    #[test]
    fn extended_pattern_parse() {
        // Note that these are globs, not regexps, so "fo?" means "fo" followed by one character.
        let exclude = Exclude::from_strings(["fo?", "ba[abc]", "[!a-z]"]).unwrap();
        assert!(exclude.matches("/foo"));
        assert!(!exclude.matches("/fo"));
        assert!(exclude.matches("/baa"));
        assert!(exclude.matches("/1"));
        assert!(!exclude.matches("/a"));
    }

    #[test]
    fn nothing_parse() {
        let exclude = Exclude::nothing();
        assert!(!exclude.matches("/a"));
    }
}
