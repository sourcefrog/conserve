// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! Command-line entry point for Conserve backups.

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

#![recursion_limit = "1024"]  // Needed by error-chain

use std::path::Path;

#[macro_use]
extern crate log;
#[macro_use]
extern crate clap;

extern crate chrono;

use chrono::Local;
use clap::{Arg, App, AppSettings, ArgMatches, SubCommand};

extern crate conserve;

use conserve::*;
use conserve::ui;


fn main() {
    let matches = make_clap().get_matches();

    let (sub_name, subm) = matches.subcommand();
    let sub_fn = match sub_name {
        "backup" => cmd_backup,
        "init" => init,
        "list-source" => list_source,
        "ls" => ls,
        "restore" => restore,
        "versions" => versions,
        _ => unimplemented!(),
    };
    let subm = subm.unwrap();

    let ui = match matches.value_of("ui").or(subm.value_of("ui")) {
        Some(ui) => ui::by_name(ui).expect("Couldn't make UI"),
        None => ui::best_ui(),
    };
    let log_level = match matches.occurrences_of("v") + subm.occurrences_of("v") {
        0 => log::LogLevelFilter::Warn,
        1 => log::LogLevelFilter::Info,
        2 => log::LogLevelFilter::Debug,
        _ => log::LogLevelFilter::max(),
    };
    let report = Report::with_ui(ui);
    report.become_logger(log_level);

    let result = sub_fn(subm, &report);

    info!("{}", report);

    if let Err(e) = result {
        show_chained_errors(e);
        std::process::exit(1)
    }
}


fn make_clap<'a, 'b>() -> clap::App<'a, 'b> {
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

    fn incomplete_arg<'a, 'b>() -> Arg<'a, 'b> {
        Arg::with_name("incomplete")
            .help("Read from incomplete (truncated) version")
            .long("incomplete")
    };

    // TODO: Allow the global options to occur even after the subcommand:
    // at the moment they have to be first.
    App::new("conserve")
        .about("A robust backup tool <http://conserve.fyi/>")
        .author(crate_authors!())
        .version(conserve::version())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(Arg::with_name("ui")
            .long("ui")
            .help("UI for progress and messages")
            .takes_value(true)
            .possible_values(&["auto", "plain", "color"]))
        .arg(Arg::with_name("v")
            .short("v")
            .multiple(true)
            .global(true)
            .help("Be more verbose (log all file names)"))
        .subcommand(SubCommand::with_name("init")
            .display_order(1)
            .about("Create a new archive")
            .arg(Arg::with_name("archive")
                .help("Path for new archive directory: \
                should either not exist or be an empty directory")
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
            .about("Copy a backup tree out of an archive")
            .arg(archive_arg())
            .arg(backup_arg())
            .arg(incomplete_arg())
            .after_help("\
                Conserve will by default refuse to restore incomplete versions, \
                to prevent you thinking you restored the whole tree when it may \
                be truncated.  You can override this with --incomplete, or \
                select an older version with --backup.")
            .arg(Arg::with_name("destination")
                .help("Restore to this new directory")
                .required(true))
            .arg(Arg::with_name("force-overwrite")
                .long("force-overwrite")
                .help("Overwrite existing destination directory")))
        .subcommand(SubCommand::with_name("versions")
            .display_order(4)
            .about("List backup versions in an archive")
            .after_help("`conserve versions` shows one version per \
                line.  For each version the output shows the version name, \
                whether it is complete, when it started, and (if complete) \
                how much time elapsed.")
            .arg(archive_arg())
            .arg(Arg::with_name("short")
                .help("List just version name without details")
                .long("short")
                .short("s")))
        .subcommand(SubCommand::with_name("ls")
            .display_order(5)
            .about("List files in a backup version")
            .arg(archive_arg())
            .arg(backup_arg())
            .arg(incomplete_arg()))
        .subcommand(SubCommand::with_name("list-source")
            .about("Recursive list files from source directory")
            .arg(Arg::with_name("source")
                .help("Source directory")
                .required(true)))
}


fn show_chained_errors(e: Error) {
    error!("{}", e);
    for suberr in e.iter().skip(1) {
        // First was already printed
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


fn cmd_backup(subm: &ArgMatches, report: &Report) -> Result<()> {
    backup(Path::new(subm.value_of("archive").unwrap()),
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


fn versions(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive_path = Path::new(subm.value_of("archive").unwrap());
    let short_output = subm.is_present("short");
    let archive = try!(Archive::open(archive_path, &report));
    for band_id in try!(archive.list_bands()) {
        if short_output {
            println!("{}", band_id);
            continue;
        }
        let band = match archive.open_band(&band_id, report) {
            Ok(band) => band,
            Err(e) => {
                warn!("Failed to open band {:?}: {:?}", band_id, e);
                continue;
            },
        };
        let info = match band.get_info(report) {
            Ok(info) => info,
            Err(e) => {
                warn!("Failed to read band tail {:?}: {:?}", band_id, e);
                continue;
            },
        };
        let is_complete_str = if info.is_closed {
            "complete"
        } else {
            "incomplete"
        };
        let start_time_str = info.start_time.with_timezone(&Local)
            .to_rfc3339();
        let duration_str = info.end_time.map_or_else(
            String::new,
            |t| format!("{}s", (t-info.start_time).num_seconds()));
        println!("{:<26} {:<10} {} {:>7}",
            band_id, is_complete_str, start_time_str, duration_str);
    }
    Ok(())
}


fn ls(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive_path = Path::new(subm.value_of("archive").unwrap());
    let archive = try!(Archive::open(archive_path, &report));
    let band_id = try!(band_id_from_match(subm));
    let band = try!(archive.open_band_or_last(&band_id, report));
    complain_if_incomplete(&band, subm.is_present("incomplete"))?;
    for i in try!(band.index_iter(report)) {
        let entry = try!(i);
        println!("{}", entry.apath);
    }
    Ok(())
}


fn restore(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive_path = Path::new(subm.value_of("archive").unwrap());
    let archive = try!(Archive::open(archive_path, &report));
    let destination_path = Path::new(subm.value_of("destination").unwrap());
    let force_overwrite = subm.is_present("force-overwrite");
    let band_id = band_id_from_match(subm)?;
    let band = archive.open_band_or_last(&band_id, report)?;
    complain_if_incomplete(&band, subm.is_present("incomplete"))?;
    conserve::Restore::new(&archive, destination_path, report)
        .force_overwrite(force_overwrite)
        .band_id(band_id)
        .run()
}


fn band_id_from_match(subm: &ArgMatches) -> Result<Option<BandId>> {
    match subm.value_of("backup") {
        Some(b) => Ok(Some(try!(BandId::from_string(b)))),
        None => Ok(None),
    }
}


fn complain_if_incomplete(band: &Band, incomplete_ok: bool) -> Result<()> {
    if !band.is_closed()? {
        if incomplete_ok {
            info!("Reading from incomplete version {}", band.id());
            Ok(())
        } else {
            Err(format!("Version {} is incomplete.  \
                (Use --incomplete to read it anyway.)", band.id()).into())
        }
    } else {
        Ok(())
    }
}
