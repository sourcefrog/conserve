// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2022 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Bands are identified by a string like `b0001-0023`, represented by a `BandId` object.

use std::fmt::{self, Write};
use std::str::FromStr;

use crate::errors::Error;

/// Identifier for a band within an archive, eg 'b0001' or 'b0001-0020'.
///
/// `BandId`s implement a total ordering `std::cmp::Ord`.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord, Hash)]
pub struct BandId {
    /// The sequence numbers at each tier.
    seqs: Vec<u32>,
}

impl BandId {
    /// Makes a new BandId from a sequence of integers.
    pub fn new(seqs: &[u32]) -> BandId {
        assert!(!seqs.is_empty());
        BandId {
            seqs: seqs.to_vec(),
        }
    }

    /// Return the origin BandId.
    #[must_use]
    pub fn zero() -> BandId {
        BandId::new(&[0])
    }

    /// Return the next BandId at the same level as self.
    #[must_use]
    pub fn next_sibling(&self) -> BandId {
        let mut next_seqs = self.seqs.clone();
        next_seqs[self.seqs.len() - 1] += 1;
        BandId::new(&next_seqs)
    }

    /// Return the previous band, unless this is zero.
    ///
    /// This is only a calculation on the band id, and the band may not be present.
    ///
    /// Currently only implemented for top-level bands.
    #[must_use]
    pub fn previous(&self) -> Option<BandId> {
        if self.seqs.len() != 1 {
            unimplemented!("BandId::previous only supported on len 1")
        }
        if self.seqs[0] == 0 {
            None
        } else {
            Some(BandId::new(&[self.seqs[0] - 1]))
        }
    }
}

impl FromStr for BandId {
    type Err = Error;

    /// Make a new BandId from a string form.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let nope = || Err(Error::InvalidVersion { version: s.into() });
        if !s.starts_with('b') {
            return nope();
        }
        let mut seqs = Vec::<u32>::new();
        for num_part in s[1..].split('-') {
            match num_part.parse::<u32>() {
                Ok(num) => seqs.push(num),
                Err(..) => return nope(),
            }
        }
        if seqs.is_empty() {
            nope()
        } else {
            Ok(BandId::new(&seqs))
        }
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
        let mut result = String::with_capacity(self.seqs.len() * 5);
        result.push('b');
        for s in &self.seqs {
            let _ = write!(result, "{s:04}-");
        }
        result.pop(); // remove the last dash
        result.shrink_to_fit();
        f.pad(&result)
    }
}

#[cfg(test)]
mod tests {
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
    fn next() {
        assert_eq!(BandId::zero().next_sibling().to_string(), "b0001");
        assert_eq!(
            BandId::new(&[2, 3]).next_sibling().to_string(),
            "b0002-0004"
        );
    }

    #[test]
    fn to_string() {
        let band_id = BandId::new(&[1, 10, 20]);
        assert_eq!(band_id.to_string(), "b0001-0010-0020");
        assert_eq!(
            BandId::new(&[1_000_000, 2_000_000]).to_string(),
            "b1000000-2000000"
        )
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
        assert_eq!(
            BandId::from_str("b0001-0100-0234").unwrap().to_string(),
            "b0001-0100-0234"
        );
    }

    #[test]
    fn format() {
        let a_bandid = BandId::from_str("b0001-0234").unwrap();
        assert_eq!(format!("{a_bandid}"), "b0001-0234");
        // Implements padding correctly
        assert_eq!(format!("{a_bandid:<15}"), "b0001-0234     ");
    }
}
