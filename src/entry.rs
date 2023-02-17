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

use serde::Serialize;
use time::OffsetDateTime;

use crate::kind::Kind;
use crate::owner::Owner;
use crate::unix_mode::UnixMode;
use crate::*;

pub trait Entry: Debug {
    fn apath(&self) -> &Apath;
    fn kind(&self) -> Kind;
    fn mtime(&self) -> OffsetDateTime;
    fn size(&self) -> Option<u64>;
    fn symlink_target(&self) -> &Option<String>;
    fn unix_mode(&self) -> UnixMode;
    fn owner(&self) -> Owner;
}

/// Summary of some kind of change to an entry from backup, diff, restore, etc.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct EntryChange {
    // TODO: Maybe give it both old and new versions of the attributes?
    pub diff_kind: DiffKind,
    pub apath: Apath,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub mtime: OffsetDateTime,
    pub owner: Owner,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
    pub unix_mode: UnixMode,
}

impl EntryChange {
    pub(crate) fn new(diff_kind: DiffKind, entry: &dyn Entry) -> Self {
        EntryChange {
            diff_kind,
            apath: entry.apath().clone(),
            size: entry.size(),
            mtime: entry.mtime(),
            owner: entry.owner().clone(),
            symlink_target: entry.symlink_target().clone(),
            unix_mode: entry.unix_mode().clone(),
        }
    }
}

/// A callback when a changed entry is visited, e.g. during a backup.
pub type ChangeCallback<'cb> = Box<dyn Fn(&EntryChange) -> Result<()> + 'cb>;
