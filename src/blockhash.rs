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
use std::fmt;
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use blake2_rfc::blake2b::Blake2bResult;
use serde::Serialize;

use crate::*;

/// The hash of a block of body data.
///
/// Stored in memory as compact bytes, but translatable to and from
/// hex strings.o
///
/// ```
/// use std::str::FromStr;
/// use conserve::blockhash::BlockHash2;
///
/// let hex_hash = concat!(
///     "00000000000000000000000000000000",
///     "00000000000000000000000000000000",
///     "00000000000000000000000000000000",
///     "00000000000000000000000000000000",);
/// let hash = BlockHash2::from_str(hex_hash)
///     .unwrap();
/// let hash2 = hash.clone();
/// assert_eq!(hash2.to_string(), hex_hash);
/// ```
#[derive(Clone, Serialize)]
#[serde(into = "String")]
#[serde(try_from = "&str")]
pub struct BlockHash2 {
    /// Binary hash.
    bin: [u8; BLAKE_HASH_SIZE_BYTES],
}

#[derive(Debug)]
pub struct BlockHashParseError {}

impl FromStr for BlockHash2 {
    type Err = BlockHashParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.len() != BLAKE_HASH_SIZE_BYTES * 2 {
            return Err(BlockHashParseError {});
        }
        let mut bin = [0; BLAKE_HASH_SIZE_BYTES];
        hex::decode_to_slice(s, &mut bin)
            .or(Err(BlockHashParseError {}))
            .and(Ok(BlockHash2 { bin }))
    }
}

impl Display for BlockHash2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(&self.bin[..]))
    }
}

impl From<BlockHash2> for String {
    fn from(hash: BlockHash2) -> String {
        hex::encode(&hash.bin[..])
    }
}

impl From<Blake2bResult> for BlockHash2 {
    fn from(hash: Blake2bResult) -> BlockHash2 {
        let mut bin = [0; BLAKE_HASH_SIZE_BYTES];
        bin.copy_from_slice(hash.as_bytes());
        BlockHash2 { bin }
    }
}

impl Ord for BlockHash2 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bin.cmp(&other.bin)
    }
}

impl PartialOrd for BlockHash2 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.bin.cmp(&other.bin))
    }
}

impl PartialEq for BlockHash2 {
    fn eq(&self, other: &Self) -> bool {
        self.bin[..] == other.bin[..]
    }
}

impl Eq for BlockHash2 {}

impl Hash for BlockHash2 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bin.hash(state);
    }
}
