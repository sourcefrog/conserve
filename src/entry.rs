// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! An entry representing a file, directory, etc, in either a
//! stored tree or local tree.

use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::*;

/// Kind of file that can be stored in the archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    /// Unknown file observed in local tree. Shouldn't be stored.
    Unknown,
}

/// Description of one archived file.
///
/// This struct is directly encoded/decoded to the json index file, and also can be constructed by
/// stat-ing (but not reading) a live file.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Path of this entry relative to the base of the backup, in `apath` form.
    pub apath: Apath,

    /// Type of file.
    pub kind: Kind,

    /// File modification time, in whole seconds past the Unix epoch.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,

    /// For stored files, the blocks holding the file contents.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addrs: Vec<blockdir::Address>,

    /// For symlinks only, the target of the symlink.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// For live files, the known size. For stored files, the size can be calculated as the sum of
    /// the blocks.
    #[serde(default)]
    #[serde(skip_serializing)]
    pub size: Option<u64>,
}

impl Entry {
    /// Return apath relative to the top of the tree.
    pub fn apath(&self) -> Apath {
        // TODO: Better to just return a reference with the same lifetime.
        self.apath.clone()
    }

    #[inline]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Return Unix-format mtime if known.
    #[inline]
    pub fn unix_mtime(&self) -> Option<u64> {
        self.mtime
    }

    /// Target of the symlink, if this is a symlink.
    #[inline]
    pub fn symlink_target(&self) -> &Option<String> {
        &self.target
    }

    /// Size of the file, if it is a file. None for directories and symlinks.
    pub fn size(&self) -> Option<u64> {
        // TODO: This is a little gross, because really there are two distinct
        // cases and we should know in advance which it is: files read from a
        // live tree should always have the `size` field populated, and files in
        // a stored tree should always have a list of addrs.
        self.size
            .or_else(|| Some(self.addrs.iter().map(|a| a.len).sum()))
    }

    /// True if the metadata supports an assumption the file contents have
    /// not changed.
    pub fn is_unchanged_from(&self, basis_entry: &Entry) -> bool {
        basis_entry.kind == self.kind
            && basis_entry.mtime.is_some()
            && basis_entry.mtime == self.mtime
            && basis_entry.size() == self.size()
    }
}
