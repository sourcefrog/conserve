// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Block hash address type.

use std::cmp::Ordering;
use std::convert::TryFrom;
use std::fmt;
use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use blake2_rfc::blake2b::{blake2b, Blake2bResult};
use serde::{Deserialize, Serialize};

use crate::*;

/// The hash of a block of body data.
///
/// Stored in memory as compact bytes, but translatable to and from
/// hex strings.
#[derive(Clone, Deserialize, Serialize)]
#[serde(into = "String")]
#[serde(try_from = "&str")]
pub struct BlockHash {
    /// Binary hash.
    bin: [u8; BLAKE_HASH_SIZE_BYTES],
}

impl BlockHash {
    pub fn hash_bytes(bytes: &[u8]) -> Self {
        BlockHash::from(blake2b(BLAKE_HASH_SIZE_BYTES, &[], bytes))
    }
}

#[derive(Debug)]
pub struct BlockHashParseError {
    rejected_string: String,
}

impl Display for BlockHashParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to parse hash string: {:?}", self.rejected_string)
    }
}

impl FromStr for BlockHash {
    type Err = BlockHashParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.len() != BLAKE_HASH_SIZE_BYTES * 2 {
            return Err(BlockHashParseError {
                rejected_string: s.to_owned(),
            });
        }
        let mut bin = [0; BLAKE_HASH_SIZE_BYTES];
        hex::decode_to_slice(s, &mut bin)
            .map_err(|_| BlockHashParseError {
                rejected_string: s.to_owned(),
            })
            .and(Ok(BlockHash { bin }))
    }
}

impl TryFrom<&str> for BlockHash {
    type Error = BlockHashParseError;

    fn try_from(s: &str) -> std::result::Result<Self, Self::Error> {
        BlockHash::from_str(s)
    }
}

impl Debug for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(&self.bin[..]))
    }
}

impl From<BlockHash> for String {
    fn from(hash: BlockHash) -> String {
        hex::encode(&hash.bin[..])
    }
}

impl From<Blake2bResult> for BlockHash {
    fn from(hash: Blake2bResult) -> BlockHash {
        let mut bin = [0; BLAKE_HASH_SIZE_BYTES];
        bin.copy_from_slice(hash.as_bytes());
        BlockHash { bin }
    }
}

impl Ord for BlockHash {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bin.cmp(&other.bin)
    }
}

impl PartialOrd for BlockHash {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.bin.cmp(&other.bin))
    }
}

impl PartialEq for BlockHash {
    fn eq(&self, other: &Self) -> bool {
        self.bin[..] == other.bin[..]
    }
}

impl Eq for BlockHash {}

impl Hash for BlockHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bin.hash(state);
    }
}

#[cfg(test)]
mod test {
    use std::collections::hash_map::DefaultHasher;

    use indoc::indoc;
    use itertools::Itertools;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn blockhash_parse_garbage_fails() {
        let r = BlockHash::from_str("garbage");
        assert_eq!(
            r.unwrap_err().to_string(),
            "Failed to parse hash string: \"garbage\""
        );
    }

    #[test]
    fn blockhash_parse_too_short_fails() {
        let r = BlockHash::from_str("01234");
        assert_eq!(
            r.unwrap_err().to_string(),
            "Failed to parse hash string: \"01234\""
        );
    }

    #[test]
    fn short_hashes_are_distinct() {
        // Reduce BlockHashes to Rust in-memory u64 hashes, and check that they remain distinct.
        let h64s = ["conserve", "backup"]
            .iter()
            .map(|s| BlockHash::hash_bytes(s.as_bytes()))
            .map(|bh| {
                let mut hasher = DefaultHasher::new();
                bh.hash(&mut hasher);
                hasher.finish()
            })
            .collect_vec();
        assert_ne!(h64s[0], h64s[1]);
    }

    #[test]
    fn unequal_hashes_are_ordered() {
        let hs = [
            BlockHash::hash_bytes(b"conserve"),
            BlockHash::hash_bytes(b"backup"),
        ];
        println!("{:#?}", hs);
        assert_ne!(hs[0], hs[1]);
        // it just happens that they come out in this order.
        assert!(hs[0] < hs[1]);
        assert!(hs[1] > hs[0]);
    }

    #[test]
    fn known_hash_values_and_reprs() {
        let hs = [
            BlockHash::hash_bytes(b"conserve"),
            BlockHash::hash_bytes(b"backup"),
        ];
        assert_eq!(
            format!("{:#?}\n", hs),
            indoc!("
                [
                    9c874eed6c5fa0588f22133d91e8cd08a657d1ee5a69591f755d1b909e530c167dcbe82d9d71e2bce803b63e988cc7dad9af5853cb438df019a916cba2876a14,
                    9e4ab7dd177800caaf4fcec323b12089100d16309b1c1d2f8f2f18310c195b6be840d0958b224b37fea065caf9bbbeda1289dda6e4a4297632c3c96161bb58c7,
                ]
            ")
        );
    }

    #[test]
    fn round_trip_string_form() {
        let h = BlockHash::hash_bytes(b"conserve");
        let hs = h.to_string();
        assert_eq!(
            hs,
            "9c874eed6c5fa0588f22133d91e8cd08a657d1ee5a69591f755d1b909e530c167dcbe82d9d71e2bce803b63e988cc7dad9af5853cb438df019a916cba2876a14"
        );
        let h2 = BlockHash::from_str(&hs).unwrap();
        assert_eq!(h, h2);
    }
}
