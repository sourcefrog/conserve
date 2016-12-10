// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Command-line entry point for Conserve backups.

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![cfg_attr(feature="bench", feature(test))] // Benchmark support currently only on nightly.

#![recursion_limit = "1024"]  // Needed by error-chain

#[macro_use]
extern crate docopt;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;

extern crate isatty;
extern crate rustc_serialize;

extern crate conserve;

use conserve::cmd;
use conserve::report;


static USAGE: &'static str = "
Conserve: a robust backup tool.
Copyright 2015, 2016 Martin Pool, GNU GPL v2.
http://conserve.fyi/

Usage:
    conserve init [options] <archive>
    conserve backup [options] <archive> <source>
    conserve list-source [options] <source>
    conserve ls [options] <archive>
    conserve restore [--force-overwrite] [options] <archive> <destination>
    conserve versions [options] <archive>
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
    cmd_list_source: bool,
    cmd_ls: bool,
    cmd_restore: bool,
    cmd_versions: bool,
    arg_archive: String,
    arg_destination: String,
    arg_source: String,
    flag_force_overwrite: bool,
    flag_no_progress: bool,
    flag_stats: bool,
}


fn main() {
    conserve::logger::establish_a_logger();
    let args: Args = docopt::Docopt::new(USAGE)
        .unwrap()
        .version(Some(conserve::VERSION.to_string()))
        .help(true)
        .decode()
        .unwrap_or_else(|e| e.exit());

    // Always turn off progress for commands that send their output to stdout:
    // easier than trying to get them not to interfere, and you should see progress
    // by the output appearing.
    let progress = isatty::stdout_isatty() &&
        !(args.flag_no_progress || args.cmd_ls || args.cmd_list_source
        || args.cmd_versions);

    let ui = if progress { conserve::ui::terminal::TermUI::new() } else { None };
    let report = report::Report::with_ui(ui);

    let result = if args.cmd_init {
        cmd::init(&args.arg_archive)
    } else if args.cmd_backup {
        cmd::backup(&args.arg_archive, &args.arg_source, &report)
    } else if args.cmd_list_source {
        cmd::list_source(&args.arg_source, &report)
    } else if args.cmd_versions {
        cmd::versions(&args.arg_archive)
    } else if args.cmd_ls {
        cmd::ls(&args.arg_archive, &report)
    } else if args.cmd_restore {
        cmd::restore(&args.arg_archive, &args.arg_destination, &report, args.flag_force_overwrite)
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
