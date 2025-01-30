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

use time::OffsetDateTime;

use crate::apath::Apath;
use crate::blockdir;
use crate::entry::{EntryTrait, EntryValue, KindMeta};
use crate::kind::Kind;
use crate::owner::Owner;
use crate::unix_mode::UnixMode;
use crate::unix_time::FromUnixAndNanos;

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

impl From<IndexEntry> for EntryValue {
    fn from(index_entry: IndexEntry) -> EntryValue {
        let kind_meta = match index_entry.kind {
            Kind::File => KindMeta::File {
                size: index_entry.addrs.iter().map(|a| a.len).sum(),
            },
            Kind::Symlink => KindMeta::Symlink {
                // TODO: Should not be fatal
                target: index_entry
                    .target
                    .expect("symlink entry should have a target"),
            },
            Kind::Dir => KindMeta::Dir,
            Kind::Unknown => KindMeta::Unknown,
        };
        EntryValue {
            apath: index_entry.apath,
            kind_meta,
            mtime: OffsetDateTime::from_unix_seconds_and_nanos(
                index_entry.mtime,
                index_entry.mtime_nanos,
            ),
            unix_mode: index_entry.unix_mode,
            owner: index_entry.owner,
        }
    }
}

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
    fn mtime(&self) -> OffsetDateTime {
        OffsetDateTime::from_unix_seconds_and_nanos(self.mtime, self.mtime_nanos)
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
}

impl IndexEntry {
    /// Copy the metadata, but not the body content, from another entry.
    ///
    /// The result has no blocks.
    pub(crate) fn metadata_from(source: &EntryValue) -> IndexEntry {
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
            mtime: mtime.unix_timestamp(),
            mtime_nanos: mtime.nanosecond(),
            unix_mode: source.unix_mode(),
            owner: source.owner().to_owned(),
        }
    }
}
