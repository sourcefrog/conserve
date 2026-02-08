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

use serde_json::json;
use jiff::Timestamp;

use crate::apath::Apath;
use crate::entry::EntryTrait;
use crate::kind::Kind;
use crate::owner::Owner;
use crate::unix_mode::UnixMode;
use crate::unix_time::timestamp_from_unix_nanos;
use crate::{blockdir, source};

/// Description of one archived file.
///
/// This struct is directly encoded/decoded to the json index file, and also can be constructed by
/// stat-ing (but not reading) a live file.
#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IndexEntry {
    /// Path of this entry relative to the base of the backup, in `apath` form.
    pub apath: Apath,

    /// Type of file.
    pub kind: Kind,

    /// File modification time, in whole seconds past the Unix epoch.
    #[serde(default)]
    pub mtime: i64,

    /// Discretionary Access Control permissions (such as read/write/execute on unix)
    #[serde(default)]
    pub unix_mode: UnixMode,

    /// User and Group names of the owners of the file
    #[serde(default, flatten, skip_serializing_if = "Owner::is_none")]
    pub owner: Owner,

    /// Fractional nanoseconds for modification time.
    ///
    /// This is zero in indexes written prior to 0.6.2, but treating it as
    /// zero is harmless - around the transition files will be seen as
    /// potentially touched.
    ///
    /// It seems moderately common that the nanos are zero, probably because
    /// the time was set by something that didn't preserve them. In that case,
    /// skip serializing.
    #[serde(default)]
    #[serde(skip_serializing_if = "crate::misc::zero_u32")]
    pub mtime_nanos: u32,

    /// For stored files, the blocks holding the file contents.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub addrs: Vec<blockdir::Address>,

    /// For symlinks only, the target of the symlink.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}
// GRCOV_EXCLUDE_STOP

impl EntryTrait for IndexEntry {
    /// Return apath relative to the top of the tree.
    fn apath(&self) -> &Apath {
        &self.apath
    }

    #[inline]
    fn kind(&self) -> Kind {
        self.kind
    }

    #[inline]
    fn mtime(&self) -> Timestamp {
        timestamp_from_unix_nanos(self.mtime, self.mtime_nanos)
    }

    /// Size of the file, if it is a file. None for directories and symlinks.
    fn size(&self) -> Option<u64> {
        Some(self.addrs.iter().map(|a| a.len).sum())
    }

    /// Target of the symlink, if this is a symlink.
    #[inline]
    fn symlink_target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    fn unix_mode(&self) -> UnixMode {
        self.unix_mode
    }

    fn owner(&self) -> &Owner {
        &self.owner
    }

    fn listing_json(&self) -> serde_json::Value {
        let mut val = json!({
            "apath": self.apath.to_string(),
            "kind": self.kind,
            "mtime": self.mtime,
            "mtime_nanos": self.mtime_nanos,
            "unix_mode": self.unix_mode,
            "user": self.owner.user,
            "group": self.owner.group,
            // omit addrs
        });
        if let Some(target) = &self.target {
            val["target"] = json!(target);
        }
        if self.kind == Kind::File {
            val["size"] = json!(self.size());
        }
        val
    }
}

impl IndexEntry {
    /// Copy the metadata, but not the body content, from another entry.
    ///
    /// The result has no blocks.
    pub(crate) fn metadata_from(source: &source::Entry) -> IndexEntry {
        let mtime = source.mtime();
        assert_eq!(
            source.symlink_target().is_some(),
            source.kind() == Kind::Symlink
        );
        IndexEntry {
            apath: source.apath().clone(),
            kind: source.kind(),
            addrs: Vec::new(),
            target: source.symlink_target().map(|t| t.to_owned()),
            mtime: mtime.as_second(),
            mtime_nanos: mtime.subsec_nanosecond() as u32,
            unix_mode: source.unix_mode(),
            owner: source.owner().to_owned(),
        }
    }
}
