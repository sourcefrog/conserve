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

//! An entry representing a file, directory, etc, in either a
//! stored tree or local tree.

use std::borrow::Borrow;
use std::fmt::Debug;

use serde::Serialize;
use time::OffsetDateTime;

use crate::*;

/// A description of an file, directory, or symlink in a tree, independent
/// of whether it's recorded in a archive (an [IndexEntry]), or
/// in a source tree.
// TODO: Maybe keep this entirely in memory and explicitly look things
// up when needed.
pub trait EntryTrait: Debug {
    fn apath(&self) -> &Apath;
    fn kind(&self) -> Kind;
    fn mtime(&self) -> OffsetDateTime;
    fn size(&self) -> Option<u64>;
    fn symlink_target(&self) -> Option<&str>;
    fn unix_mode(&self) -> UnixMode;
    fn owner(&self) -> &Owner;
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

/// An in-memory [Entry] describing a file/dir/symlink, with no addresses.
#[derive(Debug, Serialize, Clone, Eq, PartialEq)]
pub struct EntryValue {
    pub(crate) apath: Apath,

    /// Is it a file, dir, or symlink, and for files the size and for symlinks the target.
    #[serde(flatten)]
    pub(crate) kind_meta: KindMeta,

    /// Modification time.
    pub(crate) mtime: OffsetDateTime,
    pub(crate) unix_mode: UnixMode,
    #[serde(flatten)]
    pub(crate) owner: Owner,
}

impl<B: Borrow<EntryValue> + Debug> EntryTrait for B {
    fn apath(&self) -> &Apath {
        &self.borrow().apath
    }

    fn kind(&self) -> Kind {
        Kind::from(&self.borrow().kind_meta)
    }

    fn mtime(&self) -> OffsetDateTime {
        self.borrow().mtime
    }

    fn size(&self) -> Option<u64> {
        if let KindMeta::File { size } = self.borrow().kind_meta {
            Some(size)
        } else {
            None
        }
    }

    fn symlink_target(&self) -> Option<&str> {
        match &self.borrow().kind_meta {
            KindMeta::Symlink { target } => Some(target),
            _ => None,
        }
    }

    fn unix_mode(&self) -> UnixMode {
        self.borrow().unix_mode
    }

    fn owner(&self) -> &Owner {
        &self.borrow().owner
    }
}
