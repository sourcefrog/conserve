//! Conserve backup system.
//!
//! Copyright 2015, 2016 Martin Pool.
//!
//! For a description of the design and format see
//! https://github.com/sourcefrog/conserve/.

extern crate blake2_rfc;
extern crate brotli2;
#[macro_use]
extern crate log;
extern crate rustc_serialize;
extern crate term;
extern crate walkdir;

pub mod archive;
pub use archive::Archive;
pub mod backup;
pub mod band;
pub use band::BandId;
pub mod blockwriter;
pub mod logger;

/// Conserve version number as a semver string.
///
/// This is populated at compile time from `Cargo.toml`.
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
