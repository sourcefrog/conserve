// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

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

// Conserve implementation modules.
pub mod apath;
pub mod archive;
pub mod backup;
mod band;
pub mod bandid;
mod blockdir;
pub mod blockhash;
pub mod compress;
pub mod copy_tree;
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
pub(crate) mod misc;
pub mod output;
mod progress;
pub mod restore;
pub mod stats;
mod stitch;
mod stored_file;
mod stored_tree;
pub mod test_fixtures;
pub mod transport;
mod tree;
pub mod ui;
pub mod unix_time;

pub use crate::apath::Apath;
pub use crate::archive::Archive;
pub use crate::archive::DeleteOptions;
pub use crate::backup::BackupOptions;
pub use crate::backup::BackupWriter;
pub use crate::band::Band;
pub use crate::band::BandSelectionPolicy;
pub use crate::bandid::BandId;
pub use crate::blockdir::BlockDir;
pub use crate::blockhash::BlockHash;
pub use crate::copy_tree::copy_tree;
pub use crate::entry::Entry;
pub use crate::errors::Error;
pub use crate::gc_lock::GarbageCollectionLock;
pub use crate::index::{IndexBuilder, IndexEntry, IndexRead};
pub use crate::kind::Kind;
pub use crate::live_tree::{LiveEntry, LiveTree};
pub use crate::merge::{iter_merged_entries, MergedEntryKind};
pub use crate::misc::bytes_to_human_mb;
pub use crate::progress::ProgressBar;
pub use crate::restore::{RestoreOptions, RestoreTree};
pub use crate::stats::{DeleteStats, ValidateStats};
pub use crate::stored_tree::StoredTree;
pub use crate::tree::{ReadBlocks, ReadTree, TreeSize, WriteTree};

// Commonly-used external types.
pub use globset::GlobSet;

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

/// ISO timestamp, for https://docs.rs/chrono/0.4.11/chrono/format/strftime/.
const TIMESTAMP_FORMAT: &str = "%F %T";

/// Temporary files in the archive have this prefix.
const TMP_PREFIX: &str = "tmp";

/// Metadata file in the band directory.
static BAND_HEAD_FILENAME: &str = "BANDHEAD";

/// Metadata file in the band directory, for closed bands.
static BAND_TAIL_FILENAME: &str = "BANDTAIL";

/// Length of the binary content hash.
pub(crate) const BLAKE_HASH_SIZE_BYTES: usize = 64;
