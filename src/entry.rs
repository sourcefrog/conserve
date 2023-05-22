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

use std::fmt::Debug;

use time::OffsetDateTime;

use crate::kind::Kind;
use crate::owner::Owner;
use crate::unix_mode::UnixMode;
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
    fn symlink_target(&self) -> &Option<String>;
    fn unix_mode(&self) -> UnixMode;
    fn owner(&self) -> &Owner;
}

/// An in-memory [Entry] describing a file/dir/symlink, with no addresses.
#[derive(Debug)]
pub struct EntryValue {
    pub(crate) apath: Apath,
    pub(crate) kind: Kind,
    pub(crate) mtime: OffsetDateTime,
    pub(crate) size: Option<u64>,
    pub(crate) symlink_target: Option<String>,
    pub(crate) unix_mode: UnixMode,
    pub(crate) owner: Owner,
}

impl EntryTrait for EntryValue {
    fn apath(&self) -> &Apath {
        &self.apath
    }

    fn kind(&self) -> Kind {
        self.kind
    }

    fn mtime(&self) -> OffsetDateTime {
        self.mtime
    }

    fn size(&self) -> Option<u64> {
        self.size
    }

    fn symlink_target(&self) -> &Option<String> {
        &self.symlink_target
    }

    fn unix_mode(&self) -> UnixMode {
        self.unix_mode
    }

    fn owner(&self) -> &Owner {
        &self.owner
    }
}

impl EntryTrait for &EntryValue {
    fn apath(&self) -> &Apath {
        &self.apath
    }

    fn kind(&self) -> Kind {
        self.kind
    }

    fn mtime(&self) -> OffsetDateTime {
        self.mtime
    }

    fn size(&self) -> Option<u64> {
        self.size
    }

    fn symlink_target(&self) -> &Option<String> {
        &self.symlink_target
    }

    fn unix_mode(&self) -> UnixMode {
        self.unix_mode
    }

    fn owner(&self) -> &Owner {
        &self.owner
    }
}
