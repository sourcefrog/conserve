// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Command-line entry point for Conserve backups.

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

#[macro_use]
extern crate log;
#[macro_use]
extern crate docopt;

extern crate blake2_rfc;
extern crate brotli2;
extern crate rustc_serialize;
extern crate tempdir;
extern crate tempfile;
extern crate term;
extern crate time;

extern crate conserve_testsupport;

use docopt::Docopt;

// Conserve implementation modules.
mod apath;
mod archive;
mod backup;
mod band;
mod bandid;
mod block;
mod cmd;
mod index;
mod io;
mod itertools;
mod logger;
mod report;
mod sources;
#[cfg(test)]
mod testfixtures;

// Re-export important classes.
pub use archive::Archive;
pub use band::Band;
pub use bandid::BandId;
pub use report::Report;


/// Conserve version number as a semver string.
///
/// This is populated at compile time from `Cargo.toml`.
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

/// Format-compatibility version, normally the first two components of the package version.
const ARCHIVE_VERSION: &'static str = "0.3";

const BROTLI_COMPRESSION_LEVEL: u32 = 9;

static USAGE: &'static str = "
Conserve: an (incomplete) backup tool.
Copyright 2015, 2016 Martin Pool, GNU GPL v2+.
https://github.com/sourcefrog/conserve

Usage:
    conserve init [options] <archive>
    conserve backup [options] <archive> <source>
    conserve list-source [options] <source>
    conserve list-versions [options] <archive>
    conserve ls [options] <archive>
    conserve --version
    conserve --help

Options:
    --stats         Show statistics at completion.
";

#[derive(RustcDecodable)]
struct Args {
    cmd_backup: bool,
    cmd_init: bool,
    cmd_list_versions: bool,
    cmd_list_source: bool,
    cmd_ls: bool,
    arg_archive: String,
    arg_source: String,
    flag_stats: bool,
}


fn main() {
    logger::establish_a_logger();
    let mut report = report::Report::new();

    let args: Args = Docopt::new(USAGE)
        .unwrap()
        .version(Some(VERSION.to_string()))
        .help(true)
        .decode()
        .unwrap_or_else(|e| e.exit());

    let result = if args.cmd_init {
        cmd::init(&args.arg_archive)
    } else if args.cmd_backup {
        cmd::backup(&args.arg_archive, &args.arg_source, &mut report)
    } else if args.cmd_list_source {
        cmd::list_source(&args.arg_source, &mut report)
    } else if args.cmd_list_versions {
        cmd::list_versions(&args.arg_archive)
    } else if args.cmd_ls {
        cmd::ls(&args.arg_archive, &mut report)
    } else {
        unimplemented!();
    };

    if args.flag_stats {
        println!("{}", report);
    }
    if result.is_err() {
        println!("{:?}", result);
        std::process::exit(1)
    }
}
