// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Command-line entry point for Conserve backups.

extern crate conserve;
extern crate docopt;
#[macro_use]
extern crate log;
extern crate rustc_serialize;

use docopt::Docopt;
use std::path::{Path};

use conserve::archive::Archive;
use conserve::report::Report;

static USAGE: &'static str = "
Conserve: an (incomplete) backup tool.
Copyright 2015, 2016 Martin Pool, GNU GPL v2+.
https://github.com/sourcefrog/conserve

Usage:
    conserve init <archivedir>
    conserve backup <archivedir> <source>...
    conserve --version
    conserve --help
";

#[derive(RustcDecodable)]
struct Args {
    cmd_backup: bool,
    cmd_init: bool,
    arg_archivedir: String,
    arg_source: Vec<String>,
}


fn run_init(args: &Args) -> std::io::Result<()> {
    Archive::init(Path::new(&args.arg_archivedir)).and(Ok(()))
}


#[cfg_attr(test, allow(dead_code))] // https://github.com/rust-lang/rust/issues/12327
fn main() {
    conserve::logger::establish_a_logger();
    let mut report = Report::new();

    let args: Args = Docopt::new(USAGE).unwrap()
        .version(Some(conserve::VERSION.to_string()))
        .help(true)
        .decode()
        .unwrap_or_else(|e| e.exit());

    let result = if args.cmd_init {
        run_init(&args)
    } else if args.cmd_backup {
        let sources = args.arg_source.iter().map(
            |s| { Path::new(s) }
            ).collect();
        conserve::backup::run_backup(Path::new(&args.arg_archivedir), sources, &mut report)
    } else {
        panic!("unreachable?")
    };

    println!("{:?}", report);
    println!("{:?}", result);
    if result.is_err() {
        std::process::exit(1)
    }
}
