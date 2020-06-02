// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Command-line entry point for Conserve backups.

use std::path::Path;
use std::str::FromStr;

use clap::{crate_authors, App, AppSettings, Arg, ArgMatches, SubCommand};

use conserve::*;

fn main() -> conserve::Result<()> {
    let matches = make_clap().get_matches();
    ui::enable_progress(true);

    let (n, sm) = rollup_subcommands(&matches);
    let c = match n.as_str() {
        "backup" => backup,
        "debug block list" => debug_block_list,
        "debug block referenced" => debug_block_referenced,
        "debug index dump" => debug_index_dump,
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
    let result = c(sm);
    ui::clear_progress();
    if let Err(ref e) = result {
        ui::show_error(e);
        // // TODO: Perhaps always log the traceback to a log file.
        // if let Some(bt) = e.backtrace() {
        //     if std::env::var("RUST_BACKTRACE") == Ok("1".to_string()) {
        //         println!("{}", bt);
        //     }
        // }
        // Avoid Rust redundantly printing the error.
        std::process::exit(1);
    }
    // TODO: If the operation had >0 non-fatal errors, return a non-zero exit code.
    result
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
                )
                .subcommand(
                    SubCommand::with_name("index")
                        .about("Debug index")
                        .subcommand(
                            SubCommand::with_name("dump")
                                .about("Show the stored index for the given band")
                                .arg(backup_arg())
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
                        .help("Show tree sizes")
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
                        .about("Show the size of a stored tree (as it would be when restored)")
                        .arg(archive_arg())
                        .arg(backup_arg()),
                ),
        )
}

fn init(subm: &ArgMatches) -> Result<()> {
    let archive_path: std::path::PathBuf = subm
        .value_of("archive")
        .expect("'archive' arg not found")
        .parse()
        .expect("archive arg is not a path");
    Archive::create(&archive_path).and(Ok(()))?;
    ui::println(&format!("Created new archive in {:?}", archive_path));
    Ok(())
}

fn backup(subm: &ArgMatches) -> Result<()> {
    let archive = Archive::open(subm.value_of("archive").unwrap())?;
    let lt = live_tree_from_options(subm)?;
    let bw = BackupWriter::begin(&archive)?;
    let opts = CopyOptions {
        print_filenames: subm.is_present("v"),
        ..CopyOptions::default()
    };
    let copy_stats = copy_tree(&lt, bw, &opts)?;
    ui::println("Backup complete.");
    copy_stats.summarize_backup(&mut std::io::stdout());
    // ui::println(&format!("{:#?}", copy_stats));
    Ok(())
}

fn diff(subm: &ArgMatches) -> Result<()> {
    // TODO: Move this to a text-mode formatter library?
    // TODO: Consider whether the actual files have changed.
    // TODO: Summarize diff.
    // TODO: Optionally include unchanged files.
    let st = stored_tree_from_options(subm)?;
    let lt = live_tree_from_options(subm)?;
    for e in conserve::iter_merged_entries(&st, &lt)? {
        use MergedEntryKind::*;
        let ks = match e.kind {
            LeftOnly => "left",
            RightOnly => "right",
            Both => "both",
        };
        ui::println(&format!("{:<8} {}", ks, e.apath));
    }
    // TODO: Show stats.
    Ok(())
}

fn validate(subm: &ArgMatches) -> Result<()> {
    let archive = Archive::open(subm.value_of("archive").unwrap())?;
    let validate_stats = archive.validate()?;
    // ui::println(&format!("{:#?}", validate_stats));
    validate_stats.summarize(&mut std::io::stdout())?;
    Ok(())
}

fn versions(subm: &ArgMatches) -> Result<()> {
    ui::enable_progress(false);
    let archive = Archive::open(subm.value_of("archive").unwrap())?;
    let stdout = &mut std::io::stdout();
    if subm.is_present("short") {
        output::show_brief_version_list(&archive, stdout)
    } else {
        output::show_verbose_version_list(&archive, subm.is_present("sizes"), stdout)
    }
}

fn source_ls(subm: &ArgMatches) -> Result<()> {
    let lt = live_tree_from_options(subm)?;
    list_tree_contents(&lt)?;
    Ok(())
}

fn source_size(subm: &ArgMatches) -> Result<()> {
    let source = live_tree_from_options(subm)?;
    ui::set_progress_phase(&"Measuring".to_string());
    ui::println(&conserve::bytes_to_human_mb(source.size()?.file_bytes));
    Ok(())
}

fn ls(subm: &ArgMatches) -> Result<()> {
    let st = stored_tree_from_options(subm)?;
    list_tree_contents(&st)?;
    Ok(())
}

fn list_tree_contents<T: ReadTree>(tree: &T) -> Result<()> {
    // TODO: Maybe should be a specific concept in the UI.
    // TODO: Perhaps writing them one at a time causes too much locking
    // or bad buffering. Perhaps we can write to a BufferedWriter, making
    // sure that the progress bar is disabled.
    for entry in tree.iter_entries()? {
        ui::println(&entry.apath());
    }
    Ok(())
}

fn restore(subm: &ArgMatches) -> Result<()> {
    let dest = Path::new(subm.value_of("destination").unwrap());
    let st = stored_tree_from_options(subm)?;
    let rt = if subm.is_present("force-overwrite") {
        RestoreTree::create_overwrite(dest)
    } else {
        RestoreTree::create(dest)
    }?;
    let opts = CopyOptions {
        print_filenames: subm.is_present("v"),
        ..CopyOptions::default()
    };
    let copy_stats = copy_tree(&st, rt, &opts)?;
    ui::println("Restore complete.");
    copy_stats.summarize_restore(&mut std::io::stdout())?;
    // ui::println(&format!("{:#?}", copy_stats));
    Ok(())
}

fn debug_block_list(subm: &ArgMatches) -> Result<()> {
    let archive = Archive::open(subm.value_of("archive").unwrap())?;
    for b in archive.block_dir().block_names()? {
        println!("{}", b);
    }
    Ok(())
}

fn debug_block_referenced(subm: &ArgMatches) -> Result<()> {
    let archive = Archive::open(subm.value_of("archive").unwrap())?;
    for h in archive.referenced_blocks()? {
        ui::println(&h);
    }
    Ok(())
}

fn debug_index_dump(subm: &ArgMatches) -> Result<()> {
    let st = stored_tree_from_options(subm)?;
    output::show_index_json(st.band(), &mut std::io::stdout())
}

fn tree_size(subm: &ArgMatches) -> Result<()> {
    let st = stored_tree_from_options(subm)?;
    ui::set_progress_phase(&"Measuring".to_owned());
    ui::println(&bytes_to_human_mb(st.size()?.file_bytes));
    Ok(())
}

fn stored_tree_from_options(subm: &ArgMatches) -> Result<StoredTree> {
    let archive = Archive::open(subm.value_of("archive").unwrap())?;
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

fn live_tree_from_options(subm: &ArgMatches) -> Result<LiveTree> {
    Ok(LiveTree::open(&subm.value_of("source").unwrap())?
        .with_excludes(excludes_from_option(subm)?))
}

fn band_id_from_option(subm: &ArgMatches) -> Result<Option<BandId>> {
    subm.value_of("backup").map(BandId::from_str).transpose()
}

/// Make an exclusion globset from the `--exclude` option.
fn excludes_from_option(subm: &ArgMatches) -> Result<globset::GlobSet> {
    match subm.values_of("exclude") {
        Some(excludes) => excludes::from_strings(excludes),
        None => Ok(excludes::excludes_nothing()),
    }
}
