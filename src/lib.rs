// Conserve backup system.
// Copyright 2015 Martin Pool.

#[macro_use]
extern crate log;

extern crate rustc_serialize;

extern crate term;

mod archive;
pub use archive::Archive;

pub mod logger;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
