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

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, StructOpt, Subcommand};
use tracing::trace;

use conserve::backup::BackupOptions;
use conserve::ReadTree;
use conserve::RestoreOptions;
use conserve::*;

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

    /// Show debug trace to stdout.
    #[clap(long, short = 'D', global = true)]
    debug: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Copy source directory into an archive.
    Backup {
        /// Path of an existing archive.
        archive: PathBuf,
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
        archive: PathBuf,
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
        archive: PathBuf,
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
        archive: PathBuf,
    },

    /// Delete blocks unreferenced by any index.
    ///
    /// CAUTION: Do not gc while a backup is underway.
    Gc {
        /// Archive to delete from.
        archive: PathBuf,
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
        archive: PathBuf,
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
        archive: PathBuf,

        /// Skip reading and checking the content of data blocks.
        #[clap(long, short = 'q')]
        quick: bool,
        #[clap(long)]
        no_stats: bool,
    },

    /// List backup versions in an archive.
    Versions {
        archive: PathBuf,
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
    archive: Option<PathBuf>,

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
        archive: PathBuf,

        /// Backup version number.
        #[clap(long, short)]
        backup: Option<BandId>,
    },

    /// List all blocks.
    Blocks { archive: PathBuf },

    /// List all blocks referenced by any band.
    Referenced { archive: PathBuf },

    /// List garbage blocks referenced by no band.
    Unreferenced { archive: PathBuf },
}

enum ExitCode {
    Ok = 0,
    Failed = 1,
    PartialCorruption = 2,
}

impl Command {
    fn run(&self) -> Result<ExitCode> {
        let mut stdout = std::io::stdout();
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
                let stats = backup(&Archive::open_path(archive)?, source, &options)?;
                if !no_stats {
                    ui::println(&format!("Backup complete.\n{}", stats));
                }
            }
            Command::Debug(Debug::Blocks { archive }) => {
                let mut bw = BufWriter::new(stdout);
                for hash in Archive::open_path(archive)?.block_dir().block_names()? {
                    writeln!(bw, "{}", hash)?;
                }
            }
            Command::Debug(Debug::Index { archive, backup }) => {
                let st = stored_tree_from_opt(archive, backup)?;
                show::show_index_json(st.band(), &mut stdout)?;
            }
            Command::Debug(Debug::Referenced { archive }) => {
                let mut bw = BufWriter::new(stdout);
                let archive = Archive::open_path(archive)?;
                for hash in archive.referenced_blocks(&archive.list_band_ids()?)? {
                    writeln!(bw, "{}", hash)?;
                }
            }
            Command::Debug(Debug::Unreferenced { archive }) => {
                let mut bw = BufWriter::new(stdout);
                for hash in Archive::open_path(archive)?.unreferenced_blocks()? {
                    writeln!(bw, "{}", hash)?;
                }
            }
            Command::Delete {
                archive,
                backup,
                dry_run,
                break_lock,
                no_stats,
            } => {
                let stats = Archive::open_path(archive)?.delete_bands(
                    backup,
                    &DeleteOptions {
                        dry_run: *dry_run,
                        break_lock: *break_lock,
                    },
                )?;
                if !no_stats {
                    ui::println(&format!("{}", stats));
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
                show_diff(diff(&st, &lt, &options)?, &mut stdout)?;
            }
            Command::Gc {
                archive,
                dry_run,
                break_lock,
                no_stats,
            } => {
                let archive = Archive::open_path(archive)?;
                let stats = archive.delete_bands(
                    &[],
                    &DeleteOptions {
                        dry_run: *dry_run,
                        break_lock: *break_lock,
                    },
                )?;
                if !no_stats {
                    ui::println(&format!("{}", stats));
                }
            }
            Command::Init { archive } => {
                Archive::create_path(archive)?;
                ui::println(&format!("Created new archive in {:?}", &archive));
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
                        &mut stdout,
                    )?;
                } else {
                    show::show_entry_names(
                        LiveTree::open(stos.source.clone().unwrap())?
                            .iter_entries(Apath::root(), exclude)?,
                        &mut stdout,
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
                let archive = Archive::open_path(archive)?;
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
                    ui::println(&format!("Restore complete.\n{}", stats));
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
                    ui::println(&format!("{}", size));
                } else {
                    ui::println(&conserve::bytes_to_human_mb(size));
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
                let stats = Archive::open_path(archive)?.validate(&options)?;
                if !no_stats {
                    println!("{}", stats);
                }
                if stats.has_problems() {
                    ui::problem("Archive has some problems.");
                    return Ok(ExitCode::PartialCorruption);
                } else {
                    ui::println("Archive is OK.");
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
                let archive = Archive::open_path(archive)?;
                let options = ShowVersionsOptions {
                    newest_first: *newest,
                    tree_size: *sizes,
                    utc: *utc,
                    start_time: !*short,
                    backup_duration: !*short,
                };
                conserve::show_versions(&archive, &options, &mut stdout)?;
            }
        }
        Ok(ExitCode::Ok)
    }
}

fn stored_tree_from_opt(archive: &Path, backup: &Option<BandId>) -> Result<StoredTree> {
    let archive = Archive::open_path(archive)?;
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

fn main() {
    let args = Args::parse();
    ui::enable_progress(!args.no_progress && !args.debug);
    if args.debug {
        tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(tracing::Level::TRACE)
            .init();
        trace!("tracing enabled");
    }
    let result = args.command.run();
    match result {
        Err(ref e) => {
            ui::show_error(e);
            // // TODO: Perhaps always log the traceback to a log file.
            // if let Some(bt) = e.backtrace() {
            //     if std::env::var("RUST_BACKTRACE") == Ok("1".to_string()) {
            //         println!("{}", bt);
            //     }
            // }
            // Avoid Rust redundantly printing the error.
            std::process::exit(ExitCode::Failed as i32)
        }
        Ok(code) => std::process::exit(code as i32),
    }
}

#[test]
fn verify_clap() {
    use clap::CommandFactory;
    Args::command().debug_assert()
}
