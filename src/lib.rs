// Conserve backup system.

extern crate rustc_serialize;

#[macro_use]
extern crate log;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

mod archive;

pub use archive::Archive;
