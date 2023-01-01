// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2022 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! "Apaths" (for archive paths) are platform-independent relative file paths used
//! inside archive snapshots.
//!
//! The format and semantics of apaths are defined in ../doc/format.md.
//!
//! Apaths in memory are simply strings.

use std::cmp::{Ord, Ordering, PartialEq, PartialOrd};
use std::ffi::OsStr;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// An ordered archive path.
///
/// The ordering groups all the direct parents of a directory together, followed
/// by all the subdirectories.
///
/// Equal strings are equivalent to equal apaths, but the ordering is not the same as
/// string ordering.
///
/// Apaths must start with `/` and not end with `/` unless they have length 1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Apath(String);

impl Apath {
    /// True if this string is a well-formed apath.
    ///
    /// Rust strings are by contract always valid UTF-8, so to meet that requirement
    /// for apaths it's enough to use a checked conversion from bytes or an `OSString`.
    pub fn is_valid(a: &str) -> bool {
        if !a.starts_with('/') {
            return false;
        } else if a.len() == 1 {
            return true;
        }
        for part in a[1..].split('/') {
            if part.is_empty() || part == "." || part == ".." || part.contains('\0') {
                return false;
            }
        }
        true
    }

    /// Construct a new Apath for a child of `self`.
    #[must_use]
    pub fn append(&self, child_name: &str) -> Apath {
        let mut c = self.0.clone();
        if c != "/" {
            c.push('/');
        }
        c.push_str(child_name);
        Apath(c)
    }

    /// True if self is a parent directory of, or equal to, `a`.
    #[must_use]
    pub fn is_prefix_of(&self, a: &Apath) -> bool {
        let len = self.0.len();
        match len.cmp(&a.0.len()) {
            Ordering::Greater => false,
            Ordering::Equal => self.0 == a.0,
            Ordering::Less => {
                a.0.starts_with(&self.0)
                    && (self.0.ends_with('/') || a.0.chars().nth(self.0.len()) == Some('/'))
            }
        }
    }

    /// Return a PathBuf for this Apath below a tree root directory.
    #[must_use]
    pub fn below<R: Into<PathBuf>>(&self, tree_root: R) -> PathBuf {
        let mut buf: PathBuf = tree_root.into();
        buf.push(&self[1..]);
        buf
    }

    /// Construct an Apath for the root of the tree.
    #[must_use]
    pub fn root() -> Apath {
        "/".into()
    }
}

impl FromStr for Apath {
    type Err = ApathParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !Apath::is_valid(s) {
            Err(ApathParseError {})
        } else {
            Ok(Apath(s.to_owned()))
        }
    }
}

#[derive(Debug)]
pub struct DecodeFilenameError<'name> {
    name: &'name OsStr,
}

impl<'name> fmt::Display for DecodeFilenameError<'name> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Couldn't decode filename {:?}", self.name)
    }
}

#[derive(Debug)]
pub struct ApathParseError {}

impl std::error::Error for ApathParseError {}

impl fmt::Display for ApathParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Invalid apath: must have an initial slash and no ..")
    }
}

impl From<Apath> for String {
    fn from(a: Apath) -> String {
        a.0
    }
}

impl<'a> From<&'a Apath> for &'a str {
    fn from(a: &'a Apath) -> &'a str {
        &a.0
    }
}

impl<'a> From<&'a str> for Apath {
    fn from(s: &'a str) -> Apath {
        assert!(Apath::is_valid(s), "invalid apath: {s:?}");
        Apath(s.to_string())
    }
}

impl From<String> for Apath {
    fn from(s: String) -> Apath {
        assert!(Apath::is_valid(&s), "invalid apath: {s:?}");
        Apath(s)
    }
}

impl Display for Apath {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", self.0)
    }
}

/// Compare for equality an Apath to a str.
impl PartialEq<str> for Apath {
    fn eq(&self, other: &str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<&str> for Apath {
    fn eq(&self, other: &&str) -> bool {
        self.0 == **other
    }
}

impl PartialEq<Apath> for &str {
    fn eq(&self, other: &Apath) -> bool {
        other == *self
    }
}

impl Deref for Apath {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Apath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for Apath {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

/// Compare two apaths.
///
/// The ordering is _not_ the same as a simple string comparison, although
/// equal strings imply equal apaths.
impl Ord for Apath {
    fn cmp(&self, b: &Apath) -> Ordering {
        let Apath(a) = self;
        let Apath(b) = b;
        let mut ait = a.split('/');
        let mut bit = b.split('/');
        let mut oa = ait.next().expect("paths must not be empty");
        let mut ob = bit.next().expect("paths must not be empty");
        loop {
            match (ait.next(), bit.next()) {
                // Both paths end here: eg ".../aa" < ".../zz"
                (None, None) => return oa.cmp(ob),

                // If one is a direct child and the other is in a subdirectory,
                // the direct child comes first.
                // eg ".../zz" < ".../aa/bb"
                (None, Some(_bc)) => return Ordering::Less,
                (Some(_ac), None) => return Ordering::Greater,

                // Both paths have children after this point
                (Some(ac), Some(bc)) => match oa.cmp(ob) {
                    Ordering::Equal => {
                        // a/b/c/..., a/b/c/...
                        // If parents are the same and both have children keep looking.
                        oa = ac;
                        ob = bc;
                        continue;
                    }
                    // a/b/c/... < a/b/d/...
                    // Both paths have children, but the path prefixes are
                    // different.
                    other => return other,
                },
            }
        }
    }
}

impl PartialOrd for Apath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Observe Apaths and assert that they're visited in the correct order.
// GRCOV_EXCLUDE_START
#[derive(Debug, Default)]
pub struct CheckOrder {
    /// The last-seen filename, to enforce ordering.
    last_apath: Option<Apath>,
}
// GRCOV_EXCLUDE_STOP

impl CheckOrder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> CheckOrder {
        CheckOrder { last_apath: None }
    }

    pub fn check(&mut self, a: &Apath) {
        if let Some(ref last_apath) = self.last_apath {
            assert!(
                last_apath < a,
                "apaths out of order: {last_apath:?} should be before {a:?}"
            );
        }
        self.last_apath = Some(a.clone());
    }
}

#[cfg(debug_assertions)]
#[derive(Debug, Default)]
pub struct DebugCheckOrder(CheckOrder);

#[cfg(debug_assertions)]
impl DebugCheckOrder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> DebugCheckOrder {
        DebugCheckOrder(CheckOrder::new())
    }

    pub fn check(&mut self, apath: &Apath) {
        self.0.check(apath)
    }
}

#[cfg(not(debug_assertions))]
#[derive(Debug, Default)]
pub struct DebugCheckOrder();

#[cfg(not(debug_assertions))]
impl DebugCheckOrder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> DebugCheckOrder {
        DebugCheckOrder()
    }

    pub fn check(&mut self, _apath: &Apath) {}
}
