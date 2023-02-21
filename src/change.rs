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

//! A change to an entry during backup, diff, restore, etc.

use std::fmt;

use serde::Serialize;
use time::OffsetDateTime;

use crate::{Apath, Entry, Kind, Owner, Result, UnixMode};

/// Summary of some kind of change to an entry from backup, diff, restore, etc.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct EntryChange {
    pub apath: Apath,
    // TODO: Serialization as change="new".
    pub change: Change,
}

impl EntryChange {
    pub(crate) fn added(entry: &dyn Entry) -> Self {
        EntryChange {
            apath: entry.apath().clone(),
            change: Change::Added {
                added: EntryMetadata::from(entry),
            },
        }
    }

    #[allow(unused)] // Never generated in backups at the moment
    pub(crate) fn deleted(entry: &dyn Entry) -> Self {
        EntryChange {
            apath: entry.apath().clone(),
            change: Change::Deleted {
                deleted: EntryMetadata::from(entry),
            },
        }
    }

    pub(crate) fn unchanged(entry: &dyn Entry) -> Self {
        EntryChange {
            apath: entry.apath().clone(),
            change: Change::Unchanged {
                unchanged: EntryMetadata::from(entry),
            },
        }
    }

    pub(crate) fn changed(old: &dyn Entry, new: &dyn Entry) -> Self {
        debug_assert_eq!(old.apath(), new.apath());
        EntryChange {
            apath: old.apath().clone(),
            change: Change::Changed {
                old: EntryMetadata::from(old),
                new: EntryMetadata::from(new),
            },
        }
    }
}

impl fmt::Display for EntryChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.change.sigil(), self.apath)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub enum Change {
    Unchanged {
        unchanged: EntryMetadata,
    },
    Added {
        added: EntryMetadata,
    },
    Deleted {
        deleted: EntryMetadata,
    },
    Changed {
        old: EntryMetadata,
        new: EntryMetadata,
    },
}

impl Change {
    /// Return the primary metadata: the new version, unless this entry was
    /// deleted in which case the old version.
    pub fn primary_metadata(&self) -> &EntryMetadata {
        match self {
            Change::Unchanged { unchanged } => unchanged,
            Change::Added { added } => added,
            Change::Deleted { deleted } => deleted,
            Change::Changed { new, .. } => new,
        }
    }

    pub fn sigil(&self) -> char {
        // TODO: Fold DiffKind into this?
        match self {
            Change::Unchanged { .. } => '.',
            Change::Added { .. } => '+',
            Change::Deleted { .. } => '-',
            Change::Changed { .. } => '*',
        }
    }
}

/// Metadata about a changed entry other than its apath.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct EntryMetadata {
    pub mtime: OffsetDateTime,
    pub owner: Owner,
    pub kind: KindMetadata,
    pub unix_mode: UnixMode,
}

impl From<&dyn Entry> for EntryMetadata {
    fn from(entry: &dyn Entry) -> Self {
        EntryMetadata {
            kind: KindMetadata::from(entry),
            mtime: entry.mtime(),
            owner: entry.owner().clone(),
            unix_mode: entry.unix_mode().clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub enum KindMetadata {
    File { size: u64 },
    Dir,
    Symlink { target: String },
}

impl From<&dyn Entry> for KindMetadata {
    fn from(entry: &dyn Entry) -> Self {
        match entry.kind() {
            Kind::File => KindMetadata::File {
                size: entry.size().unwrap(),
            },
            Kind::Dir => KindMetadata::Dir,
            Kind::Symlink => KindMetadata::Symlink {
                target: entry.symlink_target().clone().unwrap(),
            },
            Kind::Unknown => panic!("unexpected Kind::Unknown on {:?}", entry.apath()),
        }
    }
}

/// A callback when a changed entry is visited, e.g. during a backup.
pub type ChangeCallback<'cb> = Box<dyn Fn(&EntryChange) -> Result<()> + 'cb>;
