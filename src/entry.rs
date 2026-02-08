// Conserve backup system.
// Copyright 2015-2025 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! An entry representing a file, directory, etc, in either a
//! stored tree or local tree.

use std::fmt::Debug;

use jiff::Timestamp;
use serde::Serialize;

use crate::*;

/// A description of an file, directory, or symlink in a tree, independent
/// of whether it's recorded in a archive (an [IndexEntry]), or
/// in a source tree.
// TODO: Maybe keep this entirely in memory and explicitly look things
// up when needed.
pub trait EntryTrait: Debug {
    fn apath(&self) -> &Apath;
    fn kind(&self) -> Kind;
    fn mtime(&self) -> Timestamp;
    fn size(&self) -> Option<u64>;
    fn symlink_target(&self) -> Option<&str>;
    fn unix_mode(&self) -> UnixMode;
    fn owner(&self) -> &Owner;

    fn format_ls(&self, long_listing: bool) -> String {
        if long_listing {
            format!("{} {} {}", self.unix_mode(), self.owner(), self.apath())
        } else {
            self.apath().to_string()
        }
    }

    fn listing_json(&self) -> serde_json::Value;
}

/// Per-kind metadata.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum KindMeta {
    File { size: u64 },
    Dir,
    Symlink { target: String },
    Unknown,
}

impl From<&KindMeta> for Kind {
    fn from(from: &KindMeta) -> Kind {
        match from {
            KindMeta::Dir => Kind::Dir,
            KindMeta::File { .. } => Kind::File,
            KindMeta::Symlink { .. } => Kind::Symlink,
            KindMeta::Unknown => Kind::Unknown,
        }
    }
}
