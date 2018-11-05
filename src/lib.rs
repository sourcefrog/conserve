// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Conserve backup system.
//!
//! For user documentation and an overview see http://conserve.fyi/.

extern crate blake2_rfc;
extern crate chrono;
extern crate isatty;
extern crate rayon;
extern crate rustc_serialize;
extern crate snap;
extern crate tempfile;
extern crate term;
extern crate terminal_size;
extern crate thousands;
extern crate unicode_segmentation;
extern crate walkdir;

#[cfg(test)]
extern crate spectral;

extern crate globset;

// Conserve implementation modules.
mod apath;
mod archive;
mod backup;
mod band;
mod bandid;
mod blockdir;
pub mod compress;
mod entry;
pub mod errors;
pub mod excludes;
pub mod index;
mod io;
mod jsonio;
pub mod live_tree;
pub mod output;
pub mod report;
mod restore;
mod stored_file;
mod stored_tree;
pub mod test_fixtures;
mod tree;
pub mod ui;

pub use apath::Apath;
pub use archive::Archive;
pub use backup::BackupWriter;
pub use band::Band;
pub use bandid::BandId;
pub use blockdir::BlockDir;
pub use compress::snappy::Snappy;
pub use compress::Compression;
pub use entry::{Entry, Kind};
pub use errors::*;
pub use index::{IndexBuilder, IndexEntry, ReadIndex};
pub use io::{ensure_dir_exists, list_dir, AtomicFile};
pub use live_tree::LiveTree;
pub use report::{HasReport, Report, Sizes};
pub use restore::RestoreTree;
pub use stored_tree::StoredTree;
pub use tree::{copy_tree, ReadTree, TreeSize, WriteTree};
pub use ui::UI;

// Commonly-used external types.
pub use globset::GlobSet;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn version() -> &'static str {
    VERSION
}

/// Format-compatibility version, normally the first two components of the package version.
///
/// (This might be older than the program version.)
pub const ARCHIVE_VERSION: &str = "0.5";

pub const SYMLINKS_SUPPORTED: bool = cfg!(target_family = "unix");
