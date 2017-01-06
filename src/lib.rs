// Copyright 2015, 2016 Martin Pool.

//! Conserve backup system.
//!
//! For user documentation and an overview see http://conserve.fyi/.

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![cfg_attr(feature="bench", feature(test))] // Benchmark support currently only on nightly.

#![recursion_limit = "1024"]  // Needed by error-chain

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;

extern crate blake2_rfc;
extern crate brotli2;
extern crate chrono;
extern crate isatty;
extern crate rustc_serialize;
extern crate spectral;
extern crate tempdir;
extern crate tempfile;
extern crate term;
extern crate time;

#[cfg(feature="bench")]
extern crate test;

// Conserve implementation modules.
mod apath;
mod archive;
mod backup;
mod band;
mod bandid;
mod block;
pub mod errors;
pub mod index;
mod io;
mod jsonio;
pub mod report;
mod restore;
pub mod sources;
pub mod testfixtures;
pub mod ui;

// Re-export important classes.
pub use archive::Archive;
pub use backup::backup;
pub use band::Band;
pub use bandid::BandId;
pub use block::BlockDir;
pub use report::Report;
pub use restore::Restore;

/// Conserve version number as a semver string.
///
/// This is populated at compile time by `build.rs`.
include!(concat!(env!("OUT_DIR"), "/version.rs"));
pub fn version() -> &'static str {
    semver()
}

/// Format-compatibility version, normally the first two components of the package version.
const ARCHIVE_VERSION: &'static str = "0.3";

const BROTLI_COMPRESSION_LEVEL: u32 = 4;

pub const SYMLINKS_SUPPORTED: bool = cfg!(target_family = "unix");
