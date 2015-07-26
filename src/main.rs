extern crate conserve;
extern crate docopt;
#[macro_use]
extern crate log;
extern crate rustc_serialize;

use docopt::Docopt;
use std::io::{Error};
use std::path::{Path};

static USAGE: &'static str = "
Conserve: an (incomplete) backup tool.
Copyright 2015 Martin Pool, GNU GPL v2+.
https://github.com/sourcefrog/conserve

Usage:
    conserve init <dir>
    conserve --version
    conserve --help
";

#[derive(RustcDecodable)]
struct Args {
    cmd_init: bool,
    arg_dir: String,
}


use log::{LogRecord, LogLevelFilter, LogMetadata};

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, _metadata: &LogMetadata) -> bool {
        true
    }

    fn log(&self, record: &LogRecord) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }
}


fn run_init(args: &Args) {
    match conserve::Archive::init(Path::new(&args.arg_dir)) {
        Ok(archive) => info!("Created archive in {:?}", archive.path()),
        Err(e) => error!("Failed to create archive: {}", e)
    }
}


#[cfg_attr(test, allow(dead_code))] // https://github.com/rust-lang/rust/issues/12327
fn main() {
    log::set_logger(|max_log_level| {
        max_log_level.set(LogLevelFilter::Info);
        Box::new(SimpleLogger)
    }).ok();

    let args: Args = Docopt::new(USAGE).unwrap()
        .version(Some(conserve::VERSION.to_string()))
        .help(true)
        .decode()
        .unwrap_or_else(|e| e.exit());

    if args.cmd_init {
        run_init(&args)
    } else {
        error!("unknown command?")
    }
}
