// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019 Martin Pool.

//! Command-line entry point for Conserve backups.

use std::error::Error;
use std::path::Path;

#[macro_use]
extern crate clap;

extern crate chrono;
extern crate globset;
extern crate thousands;

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use thousands::Separable;

extern crate conserve;
use conserve::*;

fn main() {
    let matches = make_clap().get_matches();
    let ui_name = matches.value_of("ui").unwrap_or("auto");
    let no_progress = matches.is_present("no-progress");
    let ui = UI::by_name(ui_name, !no_progress).expect("Couldn't make UI");
    let mut report = Report::with_ui(ui);

    let (n, sm) = rollup_subcommands(&matches);
    let c = match n.as_str() {
        "backup" => backup,
        "debug block list" => debug_block_list,
        "debug block referenced" => debug_block_referenced,
        "diff" => diff,
        "init" => init,
        "ls" => ls,
        "restore" => restore,
        "source ls" => source_ls,
        "source size" => source_size,
        "tree size" => tree_size,
        "validate" => validate,
        "versions" => versions,
        _ => panic!("unimplemented command"),
    };
    report.set_print_filenames(sm.is_present("v"));
    let result = c(sm, &report);

    report.finish();
    if matches.is_present("stats") {
        report.print(&format!("{}", report));
    }
    if let Err(e) = result {
        show_chained_errors(&report, &e);
        // TODO: Maybe show backtraces once they're available in stable
        // Rust Errors.
        std::process::exit(1)
    }
}

fn rollup_subcommands<'a>(matches: &'a ArgMatches) -> (String, &'a ArgMatches<'a>) {
    let mut sm = matches;
    let mut ns = Vec::<String>::new();
    while let (scn, Some(ssm)) = sm.subcommand() {
        if scn.is_empty() {
            break;
        };
        ns.push(scn.to_string());
        sm = ssm;
    }
    (ns.join(" "), sm)
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

    fn exclude_arg<'a, 'b>() -> Arg<'a, 'b> {
        Arg::with_name("exclude")
            .long("exclude")
            .short("e")
            .takes_value(true)
            .multiple(true)
            .number_of_values(1)
            .value_name("GLOB")
            .help("Exclude files that match the provided glob pattern")
    };

    fn incomplete_arg<'a, 'b>() -> Arg<'a, 'b> {
        Arg::with_name("incomplete")
            .help("Read from incomplete (truncated) version")
            .long("incomplete")
    };

    fn verbose_arg<'a, 'b>() -> Arg<'a, 'b> {
        Arg::with_name("v").short("v").help("Print filenames")
    };

    App::new("conserve")
        .about("A robust backup tool <https://github.com/sourcefrog/conserve/>")
        .author(crate_authors!())
        .version(conserve::version())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(
            Arg::with_name("ui")
                .long("ui")
                .short("u")
                .help("UI for progress and messages")
                .takes_value(true)
                .possible_values(&["auto", "plain", "color"]),
        )
        .arg(
            Arg::with_name("no-progress")
                .long("no-progress")
                .help("Hide progress bar"),
        )
        .arg(
            Arg::with_name("stats")
                .long("stats")
                .help("Show stats about IO, timing, and compression"),
        )
        .subcommand(
            SubCommand::with_name("debug")
                .about("Show developer-oriented information")
                .subcommand(
                    SubCommand::with_name("block")
                        .about("Debug blockdir")
                        .subcommand(
                            SubCommand::with_name("list")
                                .about("List hashes of all blocks in the blockdir")
                                .arg(Arg::with_name("archive").required(true)),
                        )
                        .subcommand(
                            SubCommand::with_name("referenced")
                                .about("List hashes of all blocks referenced by an index")
                                .arg(Arg::with_name("archive").required(true)),
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("validate")
                .about("Check whether an archive is internally consistent")
                .arg(archive_arg()),
        )
        .subcommand(
            SubCommand::with_name("init")
                .display_order(1)
                .about("Create a new archive")
                .arg(
                    Arg::with_name("archive")
                        .help(
                            "Path for new archive directory: \
                             should either not exist or be an empty directory",
                        )
                        .required(true),
                ),
        )
        .subcommand(
            SubCommand::with_name("backup")
                .display_order(2)
                .about("Copy source directory into an archive")
                .arg(archive_arg())
                .arg(
                    Arg::with_name("source")
                        .help("Backup from this directory")
                        .required(true),
                )
                .arg(exclude_arg())
                .arg(verbose_arg()),
        )
        .subcommand(
            SubCommand::with_name("diff")
                .about("Diff source against a stored tree")
                .arg(archive_arg())
                .arg(
                    Arg::with_name("source")
                        .help("Diff against this source")
                        .required(true),
                ),
        )
        .subcommand(
            SubCommand::with_name("restore")
                .display_order(3)
                .about("Copy a backup tree out of an archive")
                .arg(archive_arg())
                .arg(backup_arg())
                .arg(incomplete_arg())
                .after_help(
                    "\
                     Conserve will by default refuse to restore incomplete versions, \
                     to prevent you thinking you restored the whole tree when it may \
                     be truncated.  You can override this with --incomplete, or \
                     select an older version with --backup.",
                )
                .arg(
                    Arg::with_name("destination")
                        .help("Restore to this new directory")
                        .required(true),
                )
                .arg(
                    Arg::with_name("force-overwrite")
                        .long("force-overwrite")
                        .help("Overwrite existing destination directory"),
                )
                .arg(exclude_arg())
                .arg(verbose_arg()),
        )
        .subcommand(
            SubCommand::with_name("versions")
                .display_order(4)
                .about("List backup versions in an archive")
                .after_help(
                    "`conserve versions` shows one version per \
                     line.  For each version the output shows the version name, \
                     whether it is complete, when it started, and (if complete) \
                     how much time elapsed.",
                )
                .arg(
                    Arg::with_name("sizes")
                        .help("Show version disk sizes")
                        .long("sizes"),
                )
                .arg(archive_arg())
                .arg(
                    Arg::with_name("short")
                        .help("List just version name without details")
                        .long("short")
                        .short("s"),
                ),
        )
        .subcommand(
            SubCommand::with_name("ls")
                .display_order(5)
                .about("List files in a backup version")
                .arg(archive_arg())
                .arg(backup_arg())
                .arg(exclude_arg())
                .arg(incomplete_arg()),
        )
        .subcommand(
            SubCommand::with_name("source")
                .about("Operate on source directories")
                .subcommand(
                    SubCommand::with_name("ls")
                        .about("Recursive list files from source directory")
                        .arg(
                            Arg::with_name("source")
                                .help("Source directory")
                                .required(true),
                        )
                        .arg(exclude_arg()),
                )
                .subcommand(
                    SubCommand::with_name("size")
                        .about("Show the size of a source directory")
                        .arg(
                            Arg::with_name("source")
                                .help("Source directory")
                                .required(true),
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("tree")
                .about("Operate on stored trees")
                .subcommand(
                    SubCommand::with_name("size")
                        .about(
                            "Show the size of a stored tree (as it\
                             would be when restored)",
                        )
                        .arg(archive_arg())
                        .arg(backup_arg()),
                ),
        )
}

fn show_chained_errors(report: &Report, e: &dyn Error) {
    report.problem(&format!("{}", e));
    let mut ce = e;
    while let Some(c) = ce.source() {
        report.problem(&format!("  caused by: {}", c));
        ce = c;
    }
}

fn init(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive_path = subm.value_of("archive").expect("'archive' arg not found");
    Archive::create(archive_path).and(Ok(()))?;
    report.print(&format!("Created new archive in {}", archive_path));
    Ok(())
}

fn backup(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive = Archive::open(subm.value_of("archive").unwrap(), &report)?;
    let lt = live_tree_from_options(subm, report)?;
    let mut bw = BackupWriter::begin(&archive)?;
    copy_tree(&lt, &mut bw)?;
    report.print("Backup complete.");
    report.print(&report.borrow_counts().summary_for_backup());
    Ok(())
}

fn diff(subm: &ArgMatches, report: &Report) -> Result<()> {
    // TODO: Move this to a text-mode formatter library?
    // TODO: Consider whether the actual files have changed.
    // TODO: Summarize diff.
    // TODO: Optionally include unchanged files.
    let st = stored_tree_from_options(subm, report)?;
    let lt = live_tree_from_options(subm, report)?;
    for e in conserve::iter_merged_entries(&st, &lt, &report)? {
        use MergedEntryKind::*;
        let ee = e?;
        let ks = match ee.kind {
            LeftOnly => "left",
            RightOnly => "right",
            Both => "both",
        };
        report.print(&format!("{:<8} {}", ks, ee.apath));
    }
    // report.print(&report.borrow_counts().summary_for_backup());
    Ok(())
}

fn validate(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive = Archive::open(subm.value_of("archive").unwrap(), &report)?;
    archive.validate()?;
    report.print(&report.borrow_counts().summary_for_validate());
    Ok(())
}

fn versions(subm: &ArgMatches, report: &Report) -> Result<()> {
    use conserve::output::ShowArchive;
    let archive = Archive::open(subm.value_of("archive").unwrap(), &report)?;
    if subm.is_present("short") {
        output::ShortVersionList::default().show_archive(&archive)
    } else {
        output::VerboseVersionList::default()
            .show_sizes(subm.is_present("sizes"))
            .show_archive(&archive)
    }
}

fn source_ls(subm: &ArgMatches, report: &Report) -> Result<()> {
    let lt = live_tree_from_options(subm, report)?;
    list_tree_contents(&lt, report)?;
    Ok(())
}

fn source_size(subm: &ArgMatches, report: &Report) -> Result<()> {
    let source = live_tree_from_options(subm, report)?;
    report.set_phase("Measuring");
    report.print(&format!("{}", source.size()?.file_bytes).separate_with_commas());
    Ok(())
}

fn ls(subm: &ArgMatches, report: &Report) -> Result<()> {
    let st = stored_tree_from_options(subm, report)?;
    list_tree_contents(&st, report)?;
    Ok(())
}

fn list_tree_contents<T: ReadTree>(tree: &T, report: &Report) -> Result<()> {
    // TODO: Maybe should be a specific concept in the UI.
    for entry in tree.iter_entries(report)? {
        report.print(&entry?.apath());
    }
    Ok(())
}

fn restore(subm: &ArgMatches, report: &Report) -> Result<()> {
    let dest = Path::new(subm.value_of("destination").unwrap());
    let st = stored_tree_from_options(subm, report)?;
    let mut rt = if subm.is_present("force-overwrite") {
        RestoreTree::create_overwrite(dest, report)
    } else {
        RestoreTree::create(dest, report)
    }?;
    copy_tree(&st, &mut rt)?;
    report.print("Restore complete.");
    report.print(&report.borrow_counts().summary_for_restore());
    Ok(())
}

fn debug_block_list(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive = Archive::open(subm.value_of("archive").unwrap(), &report)?;
    for b in archive.block_dir().block_names(report)? {
        println!("{}", b);
    }
    Ok(())
}

fn debug_block_referenced(subm: &ArgMatches, report: &Report) -> Result<()> {
    let archive = Archive::open(subm.value_of("archive").unwrap(), report)?;
    for h in archive.referenced_blocks()? {
        report.print(&h);
    }
    Ok(())
}

fn tree_size(subm: &ArgMatches, report: &Report) -> Result<()> {
    let st = stored_tree_from_options(subm, report)?;
    report.set_phase("Measuring");
    report.print(&format!("{}", st.size()?.file_bytes).separate_with_commas());
    Ok(())
}

fn stored_tree_from_options(subm: &ArgMatches, report: &Report) -> Result<StoredTree> {
    let archive = Archive::open(subm.value_of("archive").unwrap(), &report)?;
    let st = match band_id_from_option(subm)? {
        None => StoredTree::open_last(&archive),
        Some(ref b) => {
            if subm.is_present("incomplete") {
                StoredTree::open_incomplete_version(&archive, b)
            } else {
                StoredTree::open_version(&archive, b)
            }
        }
    }?;
    Ok(st.with_excludes(excludes_from_option(subm)?))
}

fn live_tree_from_options(subm: &ArgMatches, report: &Report) -> Result<LiveTree> {
    Ok(LiveTree::open(&subm.value_of("source").unwrap(), &report)?
        .with_excludes(excludes_from_option(subm)?))
}

fn band_id_from_option(subm: &ArgMatches) -> Result<Option<BandId>> {
    match subm.value_of("backup") {
        Some(b) => Ok(Some(BandId::from_string(b)?)),
        None => Ok(None),
    }
}

/// Make an exclusion globset from the `--exclude` option.
fn excludes_from_option(subm: &ArgMatches) -> Result<globset::GlobSet> {
    match subm.values_of("exclude") {
        Some(excludes) => excludes::from_strings(excludes),
        None => Ok(excludes::excludes_nothing()),
    }
}
