// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! An entry representing a file, directory, etc, in either a
//! stored tree or local tree.

use std::fmt::Debug;

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

    /// Total file size.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl Entry {
    /// Return apath relative to the top of the tree.
    pub fn apath(&self) -> Apath {
        // TODO: Better to just return a reference with the same lifetime.
        self.apath.clone()
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Return Unix-format mtime if known.
    pub fn unix_mtime(&self) -> Option<u64> {
        self.mtime
    }

    /// Target of the symlink, if this is a symlink.
    pub fn symlink_target(&self) -> &Option<String> {
        &self.target
    }

    /// Size of the file, if it is a file. None for directories and symlinks.
    pub fn size(&self) -> Option<u64> {
        self.size
    }
}
