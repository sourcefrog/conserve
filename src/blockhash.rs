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

use blake2_rfc::blake2b::Blake2bResult;
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
    pub fn as_slice(&self) -> &[u8] {
        &self.bin
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
