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
extern crate walkdir;

use docopt::Docopt;

// Conserve implementation modules.
mod apath;
mod archive;
mod backup;
mod band;
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
mod version;


static USAGE: &'static str = "
Conserve: an (incomplete) backup tool.
Copyright 2015, 2016 Martin Pool, GNU GPL v2+.
https://github.com/sourcefrog/conserve

Usage:
    conserve init <archive>
    conserve backup <archive> <source>
    conserve list-source <source>
    conserve --version
    conserve --help
";

#[derive(RustcDecodable)]
struct Args {
    cmd_backup: bool,
    cmd_init: bool,
    cmd_list_source: bool,
    arg_archive: String,
    arg_source: String,
}


fn main() {
    logger::establish_a_logger();
    let mut report = report::Report::new();

    let args: Args = Docopt::new(USAGE)
        .unwrap()
        .version(Some(version::VERSION.to_string()))
        .help(true)
        .decode()
        .unwrap_or_else(|e| e.exit());

    let result = if args.cmd_init {
        cmd::init(&args.arg_archive)
    } else if args.cmd_backup {
        cmd::backup(&args.arg_archive, &args.arg_source, &mut report)
    } else if args.cmd_list_source {
        cmd::list_source(&args.arg_source, &mut report)
    } else {
        panic!("unreachable?")
    };

    println!("{:?}", report);
    println!("{:?}", result);
    if result.is_err() {
        std::process::exit(1)
    }
}
