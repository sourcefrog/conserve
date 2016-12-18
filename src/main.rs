// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Command-line entry point for Conserve backups.

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![cfg_attr(feature="bench", feature(test))] // Benchmark support currently only on nightly.

#![recursion_limit = "1024"]  // Needed by error-chain

use std::path::Path;

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;
#[macro_use]
extern crate clap;

extern crate isatty;
extern crate rustc_serialize;

use clap::{Arg, App, AppSettings, ArgMatches, SubCommand};

extern crate conserve;

use conserve::Archive;
use conserve::BandId;
use conserve::Report;
use conserve::ui;
use conserve::errors::*;


fn main() {
    fn archive_arg<'a, 'b>() -> Arg<'a, 'b> {
        Arg::with_name("archive")
            .help("Archive directory")
            .required(true)
    };

    fn backup_arg<'a, 'b>() -> Arg<'a, 'b> {
        Arg::with_name("backup")
            .help("Backup version number")
            .short("b")
            .long("backup")
            .takes_value(true)
            .value_name("VERSION")
    };

    let matches = App::new("conserve")
        .about("A robust backup tool <http://conserve.fyi/>")
        .author(crate_authors!())
        .version(crate_version!())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(Arg::with_name("stats")
            .long("stats")
            .help("Show number of operations, bytes, seconds elapsed"))
        .arg(Arg::with_name("no-progress")
            .long("no-progress")
            .help("Hide progress bars"))
        .subcommand(SubCommand::with_name("init")
            .display_order(1)
            .about("Create a new archive")
            .arg(Arg::with_name("archive")
                .help("Path for new archive directory")
                .required(true)))
        .subcommand(SubCommand::with_name("backup")
            .display_order(2)
            .about("Copy source directory into an archive")
            .arg(archive_arg())
            .arg(Arg::with_name("source")
                .help("Backup from this directory")
                .required(true)))
        .subcommand(SubCommand::with_name("restore")
            .display_order(3)
            .about("Restore files from an archive version to a new destination")
            .arg(archive_arg())
            .arg(backup_arg())
            .arg(Arg::with_name("destination")
                .help("Restore to this new directory")
                .required(true))
            .arg(Arg::with_name("force-overwrite")
                .long("force-overwrite")
                .help("Overwrite existing destination directory")))
        .subcommand(SubCommand::with_name("versions")
            .display_order(4)
            .about("List backup versions in an archive")
            .arg(archive_arg()))
        .subcommand(SubCommand::with_name("ls")
            .display_order(5)
            .about("List files in a backup version")
            .arg(backup_arg())
            .arg(archive_arg()))
        .subcommand(SubCommand::with_name("list-source")
            .about("Recursive list files from source directory")
            .arg(Arg::with_name("source")
                .help("Source directory")
                .required(true)))
        .get_matches();

    let (sub_name, subm) = matches.subcommand();
    let sub_fn = match sub_name {
        "backup" => backup,
        "init" => init,
        "list-source" => list_source,
        "ls" => ls,
        "restore" => restore,
        "versions" => versions,
        _ => unimplemented!(),
    };

    let no_progress = matches.is_present("no-progress");

    establish_a_logger();
    let ui = if no_progress { Box::new(ui::plain::PlainUI::new()) } else { ui::best_ui() };
    let report = Report::with_ui(ui);

    let result = sub_fn(subm.expect("No subcommand matches"), &report);

    if matches.is_present("stats") {
        info!("Stats:\n{}", report);
    }
    if let Err(e) = result {
        show_chained_errors(e);
        std::process::exit(1)
    }
}


fn establish_a_logger() {
    use conserve::ui::terminal::ConsoleLogger;
    use conserve::ui::plain::TextLogger;

    let logger_box: Box<log::Log> = match ConsoleLogger::new() {
        Some(l) => Box::new(l),
        None => Box::new(TextLogger::new().unwrap())
    };
    log::set_logger(|max_log_level| {
        max_log_level.set(log::LogLevelFilter::Info);
        logger_box
    }).ok();
}


fn show_chained_errors(e: Error) {
    error!("{}", e);
    for suberr in e.iter().skip(1) { // First was already printed
        error!("  {}", suberr);
    }
    if let Some(bt) = e.backtrace() {
        println!("{:?}", bt)
    }
}


fn init(subm: &ArgMatches, _report: &Report) -> Result<()> {
    let archive_path = Path::new(subm.value_of("archive").expect("'archive' arg not found"));
    Archive::init(archive_path).and(Ok(()))
}


fn backup(subm: &ArgMatches, report: &Report) -> Result<()> {
    conserve::backup(Path::new(subm.value_of("archive").unwrap()),
        Path::new(subm.value_of("source").unwrap()),
        report)
}


fn list_source(subm: &ArgMatches, report: &Report) -> Result<()> {
    let source_path = Path::new(subm.value_of("source").unwrap());
    let mut source_iter = try!(conserve::sources::iter(source_path, report));
    for entry in &mut source_iter {
        println!("{}", try!(entry).apath);
    }
    Ok(())
}


fn versions(subm: &ArgMatches, _report: &Report) -> Result<()> {
    let archive = try!(Archive::open(Path::new(subm.value_of("archive").unwrap())));
    for band_id in try!(archive.list_bands()) {
        println!("{}", band_id.as_string());
    }
    Ok(())
}


fn ls(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive = try!(Archive::open(Path::new(subm.value_of("archive").unwrap())));
    let band_id = try!(band_id_from_match(subm));
    let band = try!(archive.open_band_or_last(&band_id, report));
    for i in try!(band.index_iter(report)) {
        let entry = try!(i);
        println!("{}", entry.apath);
    }
    // TODO: Warn if the band is incomplete.
    Ok(())
}


fn restore(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive = try!(Archive::open(Path::new(subm.value_of("archive").unwrap())));
    let destination_path = Path::new(subm.value_of("destination").unwrap());
    let force_overwrite = subm.is_present("force-overwrite");
    conserve::Restore::new(&archive, destination_path, report)
        .force_overwrite(force_overwrite)
        .band_id(try!(band_id_from_match(subm)))
        .run()
}

fn band_id_from_match(subm: &ArgMatches) -> Result<Option<BandId>> {
    match subm.value_of("backup") {
        Some(b) => Ok(Some(try!(BandId::from_string(b)))),
        None => Ok(None),
    }
}
