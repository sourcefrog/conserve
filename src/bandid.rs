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

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[test]
    #[should_panic]
    fn empty_id_not_allowed() {
        BandId::new(&[]);
    }

    #[test]
    fn equality() {
        assert_eq!(BandId::new(&[1]), BandId::new(&[1]))
    }

    #[test]
    fn zero() {
        assert_eq!(BandId::zero().to_string(), "b0000");
    }

    #[test]
    fn zero_has_no_previous() {
        assert_eq!(BandId::zero().previous(), None);
    }

    #[test]
    fn previous_of_one_is_zero() {
        assert_eq!(
            BandId::zero().next_sibling().previous(),
            Some(BandId::zero())
        );
    }

    #[test]
    fn next_of_zero_is_one() {
        assert_eq!(BandId::zero().next_sibling().to_string(), "b0001");
    }

    #[test]
    fn next_of_two_is_three() {
        assert_eq!(BandId::from(2).next_sibling().to_string(), "b0003");
    }

    #[test]
    fn to_string() {
        let band_id = BandId::new(&[20]);
        assert_eq!(band_id.to_string(), "b0020");
    }

    #[test]
    fn large_value_to_string() {
        assert_eq!(BandId::new(&[2_000_000]).to_string(), "b2000000")
    }

    #[test]
    fn from_string_detects_invalid() {
        assert!(BandId::from_str("").is_err());
        assert!(BandId::from_str("hello").is_err());
        assert!(BandId::from_str("b").is_err());
        assert!(BandId::from_str("b-").is_err());
        assert!(BandId::from_str("b2-").is_err());
        assert!(BandId::from_str("b-2").is_err());
        assert!(BandId::from_str("b2-1-").is_err());
        assert!(BandId::from_str("b2--1").is_err());
        assert!(BandId::from_str("beta").is_err());
        assert!(BandId::from_str("b-eta").is_err());
        assert!(BandId::from_str("b-1eta").is_err());
        assert!(BandId::from_str("b-1-eta").is_err());
    }

    #[test]
    fn from_string_valid() {
        assert_eq!(BandId::from_str("b0001").unwrap().to_string(), "b0001");
        assert_eq!(BandId::from_str("b123456").unwrap().to_string(), "b123456");
    }

    #[test]
    fn dashes_are_no_longer_valid() {
        // Versions prior to 23.2 accepted bandids with dashes, but never
        // used them.
        let err = BandId::from_str("b0001-0100-0234").unwrap_err();
        assert_matches!(err, Error::InvalidVersion { .. });
    }

    #[test]
    fn to_string_respects_padding() {
        let s = format!("{:<10}", BandId::from(42));
        assert_eq!(s.len(), 10);
        assert_eq!(s, "b0042     ");
    }
}
