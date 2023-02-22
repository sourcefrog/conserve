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
    #[serde(flatten)]
    pub change: Change,
}

impl EntryChange {
    pub fn is_unchanged(&self) -> bool {
        self.change.is_unchanged()
    }

    pub(crate) fn diff_metadata(a: &dyn Entry, b: &dyn Entry) -> Self {
        debug_assert_eq!(a.apath(), b.apath());
        let ak = a.kind();
        // mtime is only treated as a significant change for files, because
        // the behavior on directories is not consistent between Unix and
        // Windows (and maybe not across filesystems even on Unix.)
        if ak != b.kind()
            || a.owner() != b.owner()
            || a.unix_mode() != b.unix_mode()
            || (ak == Kind::File && (a.size() != b.size() || a.mtime() != b.mtime()))
            || (ak == Kind::Symlink && (a.symlink_target() != b.symlink_target()))
        {
            EntryChange::changed(a, b)
        } else {
            EntryChange::unchanged(a)
        }
    }

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
#[serde(tag = "change")]
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
    pub fn is_unchanged(&self) -> bool {
        matches!(self, Change::Unchanged { .. })
    }

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
    // TODO: Eventually unify with LiveEntry or Entry?
    #[serde(flatten)]
    pub kind: KindMetadata,
    pub mtime: OffsetDateTime,
    #[serde(flatten)]
    pub owner: Owner,
    pub unix_mode: UnixMode,
}

impl From<&dyn Entry> for EntryMetadata {
    fn from(entry: &dyn Entry) -> Self {
        EntryMetadata {
            kind: KindMetadata::from(entry),
            mtime: entry.mtime(),
            owner: entry.owner(),
            unix_mode: entry.unix_mode(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "kind")]
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
