// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2021, 2022 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Command-line entry point for Conserve backups.

use std::error::Error;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::{Parser, StructOpt, Subcommand};
use log::{LoggingOptions, LogGuard};
use show::{show_diff, ShowVersionsOptions, show_versions};
use tracing::{ trace, error, info, warn };

use conserve::backup::BackupOptions;
use conserve::ReadTree;
use conserve::RestoreOptions;
use conserve::*;

mod log;
mod show;

#[derive(Debug, Parser)]
#[clap(
    name = "conserve",
    about = "A robust backup tool <https://github.com/sourcefrog/conserve/>",
    author,
    version
)]
struct Args {
    #[clap(subcommand)]
    command: Command,

    /// No progress bars.
    #[clap(long, short = 'P', global = true)]
    no_progress: bool,

    /// Set the log level to trace
    // TODO: Allow specifying a log level instead of only a debug flag.
    #[clap(long, short = 'D', global = true)]
    debug: bool,

    /// Path to the output log file
    #[clap(long, short = 'L', global = true)]
    log_file: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Copy source directory into an archive.
    Backup {
        /// Path of an existing archive.
        archive: String,
        /// Source directory to copy from.
        source: PathBuf,
        /// Print copied file names.
        #[clap(long, short)]
        verbose: bool,
        #[clap(long, short, number_of_values = 1)]
        exclude: Vec<String>,
        #[clap(long, short = 'E', number_of_values = 1)]
        exclude_from: Vec<String>,
        #[clap(long)]
        no_stats: bool,
    },

    #[clap(subcommand)]
    Debug(Debug),

    /// Delete backups from an archive.
    Delete {
        /// Archive to delete from.
        archive: String,
        /// Backup to delete.
        #[clap(
            long,
            short,
            multiple_occurrences(true),
            required(true),
            number_of_values(1)
        )]
        backup: Vec<BandId>,
        /// Don't actually delete, just check what could be deleted.
        #[clap(long)]
        dry_run: bool,
        /// Break a lock left behind by a previous interrupted gc operation, and then gc.
        #[clap(long)]
        break_lock: bool,
        #[clap(long)]
        no_stats: bool,
    },

    /// Compare a stored tree to a source directory.
    Diff {
        archive: String,
        source: PathBuf,
        #[clap(long, short)]
        backup: Option<BandId>,
        #[clap(long, short, number_of_values = 1)]
        exclude: Vec<String>,
        #[clap(long, short = 'E', number_of_values = 1)]
        exclude_from: Vec<String>,
        #[clap(long)]
        include_unchanged: bool,
    },

    /// Create a new archive.
    Init {
        /// Path for new archive.
        archive: String,
    },

    /// Delete blocks unreferenced by any index.
    ///
    /// CAUTION: Do not gc while a backup is underway.
    Gc {
        /// Archive to delete from.
        archive: String,
        /// Don't actually delete, just check what could be deleted.
        #[clap(long)]
        dry_run: bool,
        /// Break a lock left behind by a previous interrupted gc operation, and then gc.
        #[clap(long)]
        break_lock: bool,
        #[clap(long)]
        no_stats: bool,
    },

    /// List files in a stored tree or source directory, with exclusions.
    Ls {
        #[clap(flatten)]
        stos: StoredTreeOrSource,

        #[clap(long, short, number_of_values = 1)]
        exclude: Vec<String>,
        #[clap(long, short = 'E', number_of_values = 1)]
        exclude_from: Vec<String>,
    },

    /// Copy a stored tree to a restore directory.
    Restore {
        archive: String,
        destination: PathBuf,
        #[clap(long, short)]
        backup: Option<BandId>,
        #[clap(long, short)]
        force_overwrite: bool,
        #[clap(long, short)]
        verbose: bool,
        #[clap(long, short, number_of_values = 1)]
        exclude: Vec<String>,
        #[clap(long, short = 'E', number_of_values = 1)]
        exclude_from: Vec<String>,
        #[clap(long = "only", short = 'i', number_of_values = 1)]
        only_subtree: Option<Apath>,
        #[clap(long)]
        no_stats: bool,
    },

    /// Show the total size of files in a stored tree or source directory, with exclusions.
    Size {
        #[clap(flatten)]
        stos: StoredTreeOrSource,

        /// Count in bytes, not megabytes.
        #[clap(long)]
        bytes: bool,

        #[clap(long, short, number_of_values = 1)]
        exclude: Vec<String>,
        #[clap(long, short = 'E', number_of_values = 1)]
        exclude_from: Vec<String>,
    },

    /// Check that an archive is internally consistent.
    Validate {
        /// Path of the archive to check.
        archive: String,

        /// Skip reading and checking the content of data blocks.
        #[clap(long, short = 'q')]
        quick: bool,
        #[clap(long)]
        no_stats: bool,
    },

    /// List backup versions in an archive.
    Versions {
        archive: String,
        /// Show only version names.
        #[clap(long, short = 'q')]
        short: bool,
        /// Sort bands to show most recent first.
        #[clap(long, short = 'n')]
        newest: bool,
        /// Show size of stored trees.
        #[clap(long, short = 'z', conflicts_with = "short")]
        sizes: bool,
        /// Show times in UTC.
        #[clap(long)]
        utc: bool,
    },
}

#[derive(Debug, StructOpt)]
struct StoredTreeOrSource {
    #[clap(required_unless_present = "source")]
    archive: Option<String>,

    /// List files in a source directory rather than an archive.
    #[clap(
        long,
        short,
        conflicts_with = "archive",
        required_unless_present = "archive"
    )]
    source: Option<PathBuf>,

    #[clap(long, short, conflicts_with = "source")]
    backup: Option<BandId>,
}

/// Show debugging information.
#[derive(Debug, Subcommand)]
enum Debug {
    /// Dump the index as json.
    Index {
        /// Path of the archive to read.
        archive: String,

        /// Backup version number.
        #[clap(long, short)]
        backup: Option<BandId>,
    },

    /// List all blocks.
    Blocks { archive: String },

    /// List all blocks referenced by any band.
    Referenced { archive: String },

    /// List garbage blocks referenced by no band.
    Unreferenced { archive: String },
}

#[repr(u8)]
enum CommandExitCode {
    Ok = 0,
    Failed = 1,
    PartialCorruption = 2,
}

impl Command {
    fn run(&self) -> Result<CommandExitCode> {
        match self {
            Command::Backup {
                archive,
                source,
                verbose,
                exclude,
                exclude_from,
                no_stats,
            } => {
                let exclude = ExcludeBuilder::from_args(exclude, exclude_from)?.build()?;
                let source = &LiveTree::open(source)?;
                let options = BackupOptions {
                    print_filenames: *verbose,
                    exclude,
                    ..Default::default()
                };
                let stats = backup(&Archive::open(open_transport(archive)?)?, source, &options)?;
                if !no_stats {
                    info!("Backup complete.");
                    for line in format!("{}", stats).lines() {
                        info!("{}", line);
                    }
                }
            }
            Command::Debug(Debug::Blocks { archive }) => {
                for hash in Archive::open(open_transport(archive)?)?
                    .block_dir()
                    .block_names()?
                {
                    info!("{}", hash);
                }
            }
            Command::Debug(Debug::Index { archive, backup }) => {
                let st = stored_tree_from_opt(archive, backup)?;
                show::show_index_json(st.band())?;
            }
            Command::Debug(Debug::Referenced { archive }) => {
                let archive = Archive::open(open_transport(archive)?)?;
                for hash in archive.referenced_blocks(&archive.list_band_ids()?)? {
                    info!("{}", hash);
                }
            }
            Command::Debug(Debug::Unreferenced { archive }) => {
                for hash in Archive::open(open_transport(archive)?)?.unreferenced_blocks()? {
                    info!("{}", hash);
                }
            }
            Command::Delete {
                archive,
                backup,
                dry_run,
                break_lock,
                no_stats,
            } => {
                let stats = Archive::open(open_transport(archive)?)?.delete_bands(
                    backup,
                    &DeleteOptions {
                        dry_run: *dry_run,
                        break_lock: *break_lock,
                    },
                )?;
                if !no_stats {
                    info!("{}", stats);
                }
            }
            Command::Diff {
                archive,
                source,
                backup,
                exclude,
                exclude_from,
                include_unchanged,
            } => {
                let exclude = ExcludeBuilder::from_args(exclude, exclude_from)?.build()?;
                let st = stored_tree_from_opt(archive, backup)?;
                let lt = LiveTree::open(source)?;
                let options = DiffOptions {
                    exclude,
                    include_unchanged: *include_unchanged,
                };

                show_diff(diff(&st, &lt, &options)?)?;
            }
            Command::Gc {
                archive,
                dry_run,
                break_lock,
                no_stats,
            } => {
                let archive = Archive::open(open_transport(archive)?)?;
                let stats = archive.delete_bands(
                    &[],
                    &DeleteOptions {
                        dry_run: *dry_run,
                        break_lock: *break_lock,
                    },
                )?;
                if !no_stats {
                    info!("{}", stats);
                }
            }
            Command::Init { archive } => {
                Archive::create(open_transport(archive)?)?;
                info!("Created new archive in {:?}", &archive);
            }
            Command::Ls {
                stos,
                exclude,
                exclude_from,
            } => {
                let exclude = ExcludeBuilder::from_args(exclude, exclude_from)?.build()?;
                if let Some(archive) = &stos.archive {
                    // TODO: Option for subtree.
                    show::show_entry_names(
                        stored_tree_from_opt(archive, &stos.backup)?
                            .iter_entries(Apath::root(), exclude)?,
                    )?;
                } else {
                    show::show_entry_names(
                        LiveTree::open(stos.source.clone().unwrap())?
                            .iter_entries(Apath::root(), exclude)?,
                    )?;
                }
            }
            Command::Restore {
                archive,
                destination,
                backup,
                verbose,
                force_overwrite,
                exclude,
                exclude_from,
                only_subtree,
                no_stats,
            } => {
                let band_selection = band_selection_policy_from_opt(backup);
                let archive = Archive::open(open_transport(archive)?)?;
                let exclude = ExcludeBuilder::from_args(exclude, exclude_from)?.build()?;
                let options = RestoreOptions {
                    print_filenames: *verbose,
                    exclude,
                    only_subtree: only_subtree.clone(),
                    band_selection,
                    overwrite: *force_overwrite,
                };

                let stats = restore(&archive, destination, &options)?;
                if !no_stats {
                    info!("Restore complete.");
                    info!("{}", stats);
                }
            }
            Command::Size {
                stos,
                bytes,
                exclude,
                exclude_from,
            } => {
                let excludes = ExcludeBuilder::from_args(exclude, exclude_from)?.build()?;
                let size = if let Some(archive) = &stos.archive {
                    stored_tree_from_opt(archive, &stos.backup)?
                        .size(excludes)?
                        .file_bytes
                } else {
                    LiveTree::open(stos.source.as_ref().unwrap())?
                        .size(excludes)?
                        .file_bytes
                };
                if *bytes {
                    info!("{}", size);
                } else {
                    info!("{}", &conserve::bytes_to_human_mb(size));
                }
            }
            Command::Validate {
                archive,
                quick,
                no_stats,
            } => {
                let options = ValidateOptions {
                    skip_block_hashes: *quick,
                };
                let stats = Archive::open(open_transport(archive)?)?.validate(&options)?;
                if !no_stats {
                    println!("{}", stats);
                }
                if stats.has_problems() {
                    warn!("Archive has some problems.");
                    return Ok(CommandExitCode::PartialCorruption);
                } else {
                    info!("Archive is OK.");
                }
            }
            Command::Versions {
                archive,
                short,
                newest,
                sizes,
                utc,
            } => {
                ui::enable_progress(false);
                let archive = Archive::open(open_transport(archive)?)?;
                let options = ShowVersionsOptions {
                    newest_first: *newest,
                    tree_size: *sizes,
                    utc: *utc,
                    start_time: !*short,
                    backup_duration: !*short,
                };
                show_versions(&archive, &options)?;
            }
        }
        Ok(CommandExitCode::Ok)
    }
}

fn stored_tree_from_opt(archive_location: &str, backup: &Option<BandId>) -> Result<StoredTree> {
    let archive = Archive::open(open_transport(archive_location)?)?;
    let policy = band_selection_policy_from_opt(backup);
    archive.open_stored_tree(policy)
}

fn band_selection_policy_from_opt(backup: &Option<BandId>) -> BandSelectionPolicy {
    if let Some(band_id) = backup {
        BandSelectionPolicy::Specified(band_id.clone())
    } else {
        BandSelectionPolicy::Latest
    }
}

fn initialize_log(args: &Args) -> std::result::Result<LogGuard, String> {
    let file = args.log_file
        .as_ref()
        .map(|file| PathBuf::from_str(&file))
        .transpose()
        .map_err(|_| "Unparseable log file path".to_string())?;

    let guard = log::init(LoggingOptions{
        file,
        level: if args.debug { tracing::Level::TRACE } else { tracing::Level::INFO }
    })?;

    if args.debug {
        trace!("tracing enabled");
    }

    Ok(guard)
}

fn main() -> ExitCode {
    let args = Args::parse();
    let _log_guard = match initialize_log(&args) {
        Ok(guard) => guard,
        Err(message) => {
            eprintln!("Failed to initialize log system:");
            eprintln!("{}", message);
            return ExitCode::from(4);
        }
    };

    ui::enable_progress(!args.no_progress && !args.debug);
    let result = args.command.run();
    let exit_code = match result {
        Err(ref e) => {
            error!("{}", e.to_string());
            
            let mut cause: &dyn Error = e;
            while let Some(c) = cause.source() {
                error!("  caused by: {}", c);
                cause = c;
            }
            
            // // TODO: Perhaps always log the traceback to a log file.
            // // NOTE(WolverinDEV): May always log this as trace level?
            // if let Some(bt) = e.backtrace() {
            //     if std::env::var("RUST_BACKTRACE") == Ok("1".to_string()) {
            //         println!("{}", bt);
            //     }
            // }
            // Avoid Rust redundantly printing the error.
            ExitCode::FAILURE
        }
        Ok(code) => ExitCode::from(code as u8),
    };

    exit_code
}

#[test]
fn verify_clap() {
    use clap::CommandFactory;
    Args::command().debug_assert()
}
