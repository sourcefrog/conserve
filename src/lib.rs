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

//! Conserve backup system.

pub mod apath;
pub mod archive;
pub mod backup;
mod band;
pub mod bandid;
mod blockdir;
pub mod blockhash;
pub mod change;
pub mod compress;
mod diff;
mod entry;
pub mod errors;
pub mod excludes;
mod gc_lock;
pub mod index;
mod io;
mod jsonio;
pub mod kind;
pub mod live_tree;
mod merge;
pub mod metric_recorder;
pub mod misc;
pub mod owner;
pub mod progress;
pub mod restore;
pub mod show;
pub mod stats;
mod stitch;
mod stored_file;
mod stored_tree;
pub mod test_fixtures;
pub mod trace_counter;
pub mod transport;
mod tree;
pub mod ui;
pub mod unix_mode;
pub mod unix_time;
pub mod validate;

pub use crate::apath::Apath;
pub use crate::archive::Archive;
pub use crate::archive::DeleteOptions;
pub use crate::backup::{backup, BackupOptions};
pub use crate::band::Band;
pub use crate::band::BandSelectionPolicy;
pub use crate::bandid::BandId;
pub use crate::blockdir::BlockDir;
pub use crate::blockhash::BlockHash;
pub use crate::change::{ChangeCallback, EntryChange};
pub use crate::diff::{diff, DiffEntry, DiffKind, DiffOptions};
pub use crate::entry::Entry;
pub use crate::errors::Error;
pub use crate::excludes::Exclude;
pub use crate::gc_lock::GarbageCollectionLock;
pub use crate::index::{IndexEntry, IndexRead, IndexWriter};
pub use crate::kind::Kind;
pub use crate::live_tree::{LiveEntry, LiveTree};
pub use crate::merge::{MergeTrees, MergedEntryKind};
pub use crate::misc::bytes_to_human_mb;
pub use crate::owner::Owner;
pub use crate::restore::{restore, RestoreOptions};
pub use crate::show::{show_diff, show_versions, ShowVersionsOptions};
pub use crate::stats::{BackupStats, DeleteStats, RestoreStats};
pub use crate::stored_tree::StoredTree;
pub use crate::transport::{open_transport, Transport};
pub use crate::tree::{ReadBlocks, ReadTree, TreeSize};
pub use crate::unix_mode::UnixMode;
pub use crate::validate::ValidateOptions;

pub type Result<T> = std::result::Result<T, Error>;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version() -> &'static str {
    VERSION
}

/// Archive format-compatibility version, normally the first two components of the package version.
///
/// (This might be older than the program version.)
pub const ARCHIVE_VERSION: &str = "0.6";

pub const SYMLINKS_SUPPORTED: bool = cfg!(target_family = "unix");

/// Break blocks at this many uncompressed bytes.
pub(crate) const MAX_BLOCK_SIZE: usize = 1 << 20;

/// Maximum file size that will be combined with others rather than being stored alone.
const SMALL_FILE_CAP: u64 = 100_000;

/// Target maximum uncompressed size for combined blocks.
const TARGET_COMBINED_BLOCK_SIZE: usize = MAX_BLOCK_SIZE;

/// Temporary files in the archive have this prefix.
const TMP_PREFIX: &str = "tmp";

/// Metadata file in the band directory.
static BAND_HEAD_FILENAME: &str = "BANDHEAD";

/// Metadata file in the band directory, for closed bands.
static BAND_TAIL_FILENAME: &str = "BANDTAIL";

/// Length of the binary content hash.
pub(crate) const BLAKE_HASH_SIZE_BYTES: usize = 64;

/// A callback when an entry is visited.
pub type EntryCallback<'cb> = Box<dyn Fn(&IndexEntry) + 'cb>;
