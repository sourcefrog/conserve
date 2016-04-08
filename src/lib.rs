// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

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

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
