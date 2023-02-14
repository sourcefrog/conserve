// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Bands are identified by a string like `b0001`, represented by a [BandId] object.

use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use crate::errors::Error;

/// Identifier for a band within an archive, eg 'b0001'.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct BandId(u32);

impl BandId {
    /// Makes a new BandId from a sequence of integers.
    pub fn new(seqs: &[u32]) -> BandId {
        assert_eq!(seqs.len(), 1, "Band id should have a single element");
        BandId(seqs[0])
    }

    /// Return the origin BandId.
    #[must_use]
    pub fn zero() -> BandId {
        BandId(0)
    }

    /// Return the next BandId at the same level as self.
    #[must_use]
    pub fn next_sibling(&self) -> BandId {
        BandId(self.0 + 1)
    }

    /// Return the previous band, unless this is zero.
    ///
    /// This is only a calculation on the band id, and the band may not be present.
    ///
    /// Currently only implemented for top-level bands.
    #[must_use]
    pub fn previous(&self) -> Option<BandId> {
        if self.0 == 0 {
            None
        } else {
            Some(BandId(self.0 - 1))
        }
    }
}

impl FromStr for BandId {
    type Err = Error;

    /// Make a new BandId from a string form.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Some(num) = s.strip_prefix('b') {
            if let Ok(num) = num.parse::<u32>() {
                return Ok(BandId(num));
            }
        }
        Err(Error::InvalidVersion { version: s.into() })
    }
}

impl From<u32> for BandId {
    fn from(value: u32) -> Self {
        BandId(value)
    }
}

impl fmt::Display for BandId {
    /// Returns the string representation of this BandId.
    ///
    /// Bands have an id which is a sequence of one or more non-negative integers.
    /// This is externally represented as a string like `b0001-0010`, which becomes
    /// their directory name in the archive.
    ///
    /// Numbers are zero-padded to what should normally be a reasonable length,
    /// but they can be longer.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(&format!("b{:0>4}", self.0))
    }
}
