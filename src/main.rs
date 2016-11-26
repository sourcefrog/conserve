// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Command-line entry point for Conserve backups.

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

#![recursion_limit = "1024"]  // Needed by error-chain

#[macro_use]
extern crate docopt;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;

extern crate blake2_rfc;
extern crate brotli2;
extern crate rustc_serialize;
extern crate spectral;
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
mod errors;
mod index;
mod io;
mod logger;
mod report;
mod restore;
mod sources;
#[cfg(test)]
mod testfixtures;
mod ui;

// Re-export important classes.
pub use archive::Archive;
pub use band::Band;
pub use bandid::BandId;
pub use block::BlockDir;
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
    conserve restore [options] <archive> <destination>
    conserve --version
    conserve --help

Options:
    --stats         Show statistics at completion.
    --no-progress   No progress bar.
";

#[derive(RustcDecodable)]
struct Args {
    cmd_backup: bool,
    cmd_init: bool,
    cmd_list_versions: bool,
    cmd_list_source: bool,
    cmd_ls: bool,
    cmd_restore: bool,
    arg_archive: String,
    arg_destination: String,
    arg_source: String,
    flag_no_progress: bool,
    flag_stats: bool,
}


fn main() {
    logger::establish_a_logger();
    let args: Args = Docopt::new(USAGE)
        .unwrap()
        .version(Some(VERSION.to_string()))
        .help(true)
        .decode()
        .unwrap_or_else(|e| e.exit());

    // Always turn off progress for commands that send their output to stdout:
    // easier than trying to get them not to interfere, and you should see progress
    // by the output appearing.
    let progress = !(args.flag_no_progress || args.cmd_ls || args.cmd_list_source
        || args.cmd_list_versions);

    let ui = if progress { ui::terminal::TermUI::new() } else { None };
    let report = report::Report::with_ui(ui);

    let result = if args.cmd_init {
        cmd::init(&args.arg_archive)
    } else if args.cmd_backup {
        cmd::backup(&args.arg_archive, &args.arg_source, &report)
    } else if args.cmd_list_source {
        cmd::list_source(&args.arg_source, &report)
    } else if args.cmd_list_versions {
        cmd::list_versions(&args.arg_archive)
    } else if args.cmd_ls {
        cmd::ls(&args.arg_archive, &report)
    } else if args.cmd_restore {
        cmd::restore(&args.arg_archive, &args.arg_destination, &report)
    } else {
        unimplemented!();
    };

    if args.flag_stats {
        info!("Stats:\n{}", report);
    }
    if let Err(e) = result {
        error!("{}", e);
        for suberr in e.iter().skip(1) { // First was already printed
            error!("  {}", suberr);
        }
        if let Some(bt) = e.backtrace() {
            println!("{:?}", bt)
        }
        std::process::exit(1)
    }
}
