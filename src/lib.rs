// Copyright 2015, 2016, 2017 Martin Pool.

//! Conserve backup system.
//!
//! For user documentation and an overview see http://conserve.fyi/.

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

#![recursion_limit = "1024"] // Needed by error-chain

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;

extern crate blake2_rfc;
extern crate chrono;
extern crate isatty;
extern crate rustc_serialize;
extern crate snap;
extern crate tempdir;
extern crate tempfile;
extern crate term;

#[cfg(test)]
extern crate spectral;

extern crate globset;


// Conserve implementation modules.
mod apath;
mod archive;
mod backup;
mod band;
mod bandid;
mod block;
mod entry;
pub mod excludes;
pub mod compress;
pub mod errors;
pub mod index;
mod io;
mod jsonio;
pub mod live_tree;
pub mod report;
mod restore;
mod stored_file;
mod stored_tree;
pub mod test_fixtures;
pub mod ui;

pub use archive::Archive;
pub use apath::Apath;
pub use backup::{BackupOptions, make_backup};
pub use band::Band;
pub use bandid::BandId;
pub use block::BlockDir;
pub use compress::Compression;
pub use compress::snappy::Snappy;
pub use entry::{Entry, Kind};
pub use errors::*;
pub use io::{AtomicFile, ensure_dir_exists};
pub use index::{IndexBuilder, IndexEntry};
pub use live_tree::LiveTree;
pub use report::{Report, Sizes};
pub use restore::{RestoreOptions, restore_tree};
pub use stored_tree::StoredTree;
pub use ui::UI;


/// Conserve version number as a semver string.
///
/// This is populated at compile time by `build.rs`.
include!(concat!(env!("OUT_DIR"), "/version.rs"));
pub fn version() -> &'static str {
    semver()
}

/// Format-compatibility version, normally the first two components of the package version.
pub const ARCHIVE_VERSION: &'static str = "0.4";

pub const SYMLINKS_SUPPORTED: bool = cfg!(target_family = "unix");
