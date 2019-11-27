// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019 Martin Pool.

//! Bands are identified by a string like `b0001-0023`, represented by a `BandId` object.

use std::fmt;

use crate::errors::*;

/// Identifier for a band within an archive, eg 'b0001' or 'b0001-0020'.
///
/// `BandId`s implement a total ordering `std::cmp::Ord`.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord)]
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
    pub fn zero() -> BandId {
        BandId::new(&[0])
    }

    /// Return the next BandId at the same level as self.
    pub fn next_sibling(&self) -> BandId {
        let mut next_seqs = self.seqs.clone();
        next_seqs[self.seqs.len() - 1] += 1;
        BandId::new(&next_seqs)
    }

    /// Make a new BandId from a string form.
    pub fn from_string(s: &str) -> Result<BandId> {
        let nope = Err(Error::InvalidVersion);
        if !s.starts_with('b') {
            return nope;
        }
        let mut seqs = Vec::<u32>::new();
        for num_part in s[1..].split('-') {
            match num_part.parse::<u32>() {
                Ok(num) => seqs.push(num),
                Err(..) => return nope,
            }
        }
        if seqs.is_empty() {
            nope
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
        result.push_str("b");
        for s in &self.seqs {
            result.push_str(&format!("{:04}-", s));
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
            BandId::new(&[1000000, 2000000]).to_string(),
            "b1000000-2000000"
        )
    }

    #[test]
    fn from_string_detects_invalid() {
        assert!(BandId::from_string("").is_err());
        assert!(BandId::from_string("hello").is_err());
        assert!(BandId::from_string("b").is_err());
        assert!(BandId::from_string("b-").is_err());
        assert!(BandId::from_string("b2-").is_err());
        assert!(BandId::from_string("b-2").is_err());
        assert!(BandId::from_string("b2-1-").is_err());
        assert!(BandId::from_string("b2--1").is_err());
        assert!(BandId::from_string("beta").is_err());
        assert!(BandId::from_string("b-eta").is_err());
        assert!(BandId::from_string("b-1eta").is_err());
        assert!(BandId::from_string("b-1-eta").is_err());
    }

    #[test]
    fn from_string_valid() {
        assert_eq!(BandId::from_string("b0001").unwrap().to_string(), "b0001");
        assert_eq!(
            BandId::from_string("b123456").unwrap().to_string(),
            "b123456"
        );
        assert_eq!(
            BandId::from_string("b0001-0100-0234").unwrap().to_string(),
            "b0001-0100-0234"
        );
    }

    #[test]
    fn format() {
        let a_bandid = BandId::from_string("b0001-0234").unwrap();
        assert_eq!(format!("{}", a_bandid), "b0001-0234");
        // Implements padding correctly
        assert_eq!(format!("{:<15}", a_bandid), "b0001-0234     ");
    }
}
