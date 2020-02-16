// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Conserve backup system.

// Conserve implementation modules.
mod apath;
mod archive;
mod backup;
mod band;
mod bandid;
mod blockdir;
pub mod compress;
mod copy_tree;
mod entry;
pub mod errors;
pub mod excludes;
pub mod index;
mod io;
mod jsonio;
pub mod live_tree;
mod merge;
pub(crate) mod misc;
pub mod output;
pub mod report;
mod restore;
mod stored_file;
mod stored_tree;
pub mod test_fixtures;
mod tree;
pub mod ui;

pub use crate::apath::Apath;
pub use crate::archive::Archive;
pub use crate::backup::BackupWriter;
pub use crate::band::Band;
pub use crate::bandid::BandId;
pub use crate::blockdir::BlockDir;
pub use crate::compress::snappy::Snappy;
pub use crate::compress::Compression;
pub use crate::copy_tree::copy_tree;
pub use crate::entry::{Entry, Kind};
pub use crate::errors::*;
pub use crate::index::{IndexBuilder, IndexEntry, ReadIndex};
pub use crate::io::{ensure_dir_exists, list_dir, AtomicFile};
pub use crate::live_tree::{LiveEntry, LiveTree};
pub use crate::merge::{iter_merged_entries, MergedEntryKind};
pub use crate::misc::bytes_to_human_mb;
pub use crate::report::{HasReport, Report, Sizes};
pub use crate::restore::RestoreTree;
pub use crate::stored_tree::StoredTree;
pub use crate::tree::{ReadBlocks, ReadTree, TreeSize, WriteTree};
pub use crate::ui::UI;

// Commonly-used external types.
pub use globset::GlobSet;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version() -> &'static str {
    VERSION
}

/// Format-compatibility version, normally the first two components of the package version.
///
/// (This might be older than the program version.)
pub const ARCHIVE_VERSION: &str = "0.6";

pub const SYMLINKS_SUPPORTED: bool = cfg!(target_family = "unix");

/// Break blocks at this many uncompressed bytes.
pub(crate) const MAX_BLOCK_SIZE: usize = 1 << 20;
