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

//! Conserve backup system.

pub mod apath;
pub mod archive;
pub mod backup;
mod band;
pub mod bandid;
pub mod blockdir;
pub mod blockhash;
pub mod change;
pub mod compress;
pub mod counters;
mod diff;
pub mod entry;
pub mod errors;
pub mod excludes;
pub mod flags;
mod gc_lock;
mod hunk_index;
pub mod index;
mod io;
mod jsonio;
pub mod kind;
mod merge;
pub mod misc;
pub mod monitor;
mod mount;
pub mod owner;
pub mod restore;
pub mod show;
pub mod source;
pub mod stats;
mod stored_tree;
pub mod termui;
pub mod test_fixtures;
pub mod transport;
mod tree;
pub mod unix_mode;
pub mod unix_time;
pub mod validate;

pub use crate::apath::Apath;
pub use crate::archive::Archive;
pub use crate::archive::DeleteOptions;
pub use crate::backup::{backup, BackupOptions, BackupStats};
pub use crate::band::{Band, BandSelectionPolicy};
pub use crate::bandid::BandId;
pub use crate::blockhash::BlockHash;
pub use crate::change::{ChangeCallback, EntryChange};
pub use crate::diff::{diff, DiffOptions};
pub use crate::entry::EntryTrait;
pub use crate::errors::Error;
pub use crate::excludes::Exclude;
pub use crate::gc_lock::GarbageCollectionLock;
pub use crate::index::{entry::IndexEntry, IndexRead, IndexWriter};
pub use crate::kind::Kind;
pub use crate::merge::MergeTrees;
pub use crate::misc::bytes_to_human_mb;
pub use crate::mount::{mount, MountOptions};
pub use crate::owner::Owner;
pub use crate::restore::{restore, RestoreOptions};
pub use crate::show::{show_versions, ShowVersionsOptions};
pub use crate::source::SourceTree;
pub use crate::stats::DeleteStats;
pub use crate::stored_tree::StoredTree;
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

/// Metadata file in the band directory.
static BAND_HEAD_FILENAME: &str = "BANDHEAD";

/// Metadata file in the band directory, for closed bands.
static BAND_TAIL_FILENAME: &str = "BANDTAIL";

/// Length of the binary content hash.
pub(crate) const BLAKE_HASH_SIZE_BYTES: usize = 64;
