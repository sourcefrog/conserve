// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

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

use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use clap::builder::{styling, Styles};
use clap::{Parser, Subcommand};
use conserve::change::Change;
use rayon::prelude::ParallelIterator;
use time::UtcOffset;
#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn, Level};

use conserve::termui::{enable_tracing, TermUiMonitor, TraceTimeStyle};
use conserve::*;
use transport::Transport2;

/// Local timezone offset, calculated once at startup, to avoid issues about
/// looking at the environment once multiple threads are running.
static LOCAL_OFFSET: RwLock<UtcOffset> = RwLock::new(UtcOffset::UTC);

#[mutants::skip] // only visual effects, not worth testing
fn clap_styles() -> Styles {
    styling::Styles::styled()
        .header(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .usage(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .literal(styling::AnsiColor::Blue.on_default() | styling::Effects::BOLD)
        .placeholder(styling::AnsiColor::Cyan.on_default())
}

#[derive(Debug, Parser)]
#[command(author, about, version, styles(clap_styles()))]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// No progress bars.
    #[arg(long, short = 'P', global = true)]
    no_progress: bool,

    /// Show debug trace to stdout.
    #[arg(long, short = 'D', global = true)]
    debug: bool,

    /// Control timestamps prefixes on stderr.
    #[arg(long, value_enum, global = true, default_value_t = TraceTimeStyle::None)]
    trace_time: TraceTimeStyle,

    /// Append a json formatted log to this file.
    #[arg(long, global = true)]
    log_json: Option<PathBuf>,

    /// Write metrics to this file: deprecated and ignored.
    #[arg(long, global = true, hide = true)]
    metrics_json: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Copy source directory into an archive.
    Backup {
        /// Path of an existing archive.
        archive: String,
        /// Source directory to copy from.
        source: PathBuf,
        /// Write a list of changes to this file.
        #[arg(long)]
        changes_json: Option<PathBuf>,
        /// Print copied file names.
        #[arg(long, short)]
        verbose: bool,
        #[arg(long, short)]
        exclude: Vec<String>,
        /// Read a list of globs to exclude from this file.
        #[arg(long, short = 'E')]
        exclude_from: Vec<String>,
        /// Don't print statistics after the backup completes.
        #[arg(long)]
        no_stats: bool,
        /// Show permissions, owner, and group in verbose output.
        #[arg(long, short = 'l')]
        long_listing: bool,
    },

    #[command(subcommand)]
    Debug(Debug),

    /// Delete backups from an archive.
    Delete {
        /// Archive to delete from.
        archive: String,
        /// Backup to delete, as an id like 'b1'. May be repeated with commas.
        #[arg(long, short, value_delimiter = ',', required(true))]
        backup: Vec<BandId>,
        /// Don't actually delete, just check what could be deleted.
        #[arg(long)]
        dry_run: bool,
        /// Break a lock left behind by a previous interrupted gc operation, and then gc.
        #[arg(long)]
        break_lock: bool,
        #[arg(long)]
        no_stats: bool,
    },

    /// Compare a stored tree to a source directory.
    Diff {
        /// Path or URL of an existing archive.
        archive: String,
        /// Source directory to compare to.
        source: PathBuf,
        /// Select the version from the archive to compare: by default, the latest.
        #[arg(long, short)]
        backup: Option<BandId>,
        #[arg(long, short)]
        exclude: Vec<String>,
        #[arg(long, short = 'E')]
        exclude_from: Vec<String>,
        #[arg(long)]
        include_unchanged: bool,

        /// Print the diff as json.
        #[arg(long, short)]
        json: bool,
    },

    /// Create a new archive.
    Init {
        /// Path for new archive.
        archive: String,
    },

    /// Delete blocks unreferenced by any index.
    Gc {
        /// Archive to delete from.
        archive: String,
        /// Don't actually delete, just check what could be deleted.
        #[arg(long)]
        dry_run: bool,
        /// Break a lock left behind by a previous interrupted gc operation, and then gc.
        #[arg(long)]
        break_lock: bool,
        #[arg(long)]
        no_stats: bool,
    },

    /// List files in a stored tree or source directory, with exclusions.
    Ls {
        #[command(flatten)]
        stos: StoredTreeOrSource,

        #[arg(long, short)]
        exclude: Vec<String>,

        #[arg(long, short = 'E')]
        exclude_from: Vec<String>,

        /// Print entries as json.
        #[arg(long, short)]
        json: bool,

        /// Show permissions, owner, and group.
        #[arg(short = 'l')]
        long_listing: bool,
    },

    /// Copy a stored tree to a restore directory.
    Restore {
        archive: String,
        destination: PathBuf,
        #[arg(long, short)]
        backup: Option<BandId>,
        /// Write a list of restored files to this json file.
        #[arg(long)]
        changes_json: Option<PathBuf>,
        #[arg(long, short)]
        force_overwrite: bool,
        #[arg(long, short)]
        verbose: bool,
        #[arg(long, short)]
        exclude: Vec<String>,
        #[arg(long, short = 'E')]
        exclude_from: Vec<String>,
        #[arg(long = "only", short = 'i')]
        only_subtree: Option<Apath>,
        #[arg(long)]
        no_stats: bool,
        /// Show permissions, owner, and group in verbose output.
        #[arg(long, short = 'l')]
        long_listing: bool,
    },

    /// Show the total size of files in a stored tree or source directory, with exclusions.
    Size {
        #[command(flatten)]
        stos: StoredTreeOrSource,

        /// Count in bytes, not megabytes.
        #[arg(long)]
        bytes: bool,

        #[arg(long, short)]
        exclude: Vec<String>,
        #[arg(long, short = 'E')]
        exclude_from: Vec<String>,
    },

    /// Check that an archive is internally consistent.
    Validate {
        /// Path of the archive to check.
        archive: String,

        /// Skip reading and checking the content of data blocks.
        #[arg(long, short = 'q')]
        quick: bool,
        #[arg(long)]
        no_stats: bool,
    },

    /// List backup versions in an archive.
    Versions {
        archive: String,
        /// Show only version names.
        #[arg(long, short = 'q')]
        short: bool,
        /// Sort bands to show most recent first.
        #[arg(long, short = 'n')]
        newest: bool,
        /// Show size of stored trees.
        #[arg(long, short = 'z', conflicts_with = "short")]
        sizes: bool,
        /// Show times in UTC.
        #[arg(long)]
        utc: bool,
    },
}

#[derive(Debug, Parser)]
struct StoredTreeOrSource {
    #[arg(required_unless_present = "source")]
    archive: Option<String>,

    /// List files in a source directory rather than an archive.
    #[arg(
        long,
        short,
        conflicts_with = "archive",
        required_unless_present = "archive"
    )]
    source: Option<PathBuf>,

    #[arg(long, short, conflicts_with = "source")]
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
        #[arg(long, short)]
        backup: Option<BandId>,
    },

    /// List all blocks.
    Blocks { archive: String },

    /// List all blocks referenced by any band.
    Referenced { archive: String },

    /// List garbage blocks referenced by no band.
    Unreferenced { archive: String },
}

enum ExitCode {
    Success = 0,
    Failure = 1,
    NonFatalErrors = 2,
}

impl std::process::Termination for ExitCode {
    fn report(self) -> std::process::ExitCode {
        (self as u8).into()
    }
}

impl Command {
    fn run(&self, monitor: Arc<TermUiMonitor>) -> Result<ExitCode> {
        let mut stdout = std::io::stdout();
        match self {
            Command::Backup {
                archive,
                changes_json,
                exclude,
                exclude_from,
                long_listing,
                no_stats,
                source,
                verbose,
            } => {
                let options = BackupOptions {
                    exclude: Exclude::from_patterns_and_files(exclude, exclude_from)?,
                    change_callback: make_change_callback(
                        *verbose,
                        *long_listing,
                        &changes_json.as_deref(),
                    )?,
                    ..Default::default()
                };
                let stats = backup(
                    &Archive::open(Transport2::new(archive)?)?,
                    source,
                    &options,
                    monitor,
                )?;
                if !no_stats {
                    info!("Backup complete.\n{stats}");
                }
            }
            Command::Debug(Debug::Blocks { archive }) => {
                let mut bw = BufWriter::new(stdout);
                for hash in Archive::open(Transport2::new(archive)?)?
                    .block_dir()
                    .blocks(monitor)?
                    .collect::<Vec<BlockHash>>()
                    .into_iter()
                {
                    writeln!(bw, "{hash}")?;
                }
            }
            Command::Debug(Debug::Index { archive, backup }) => {
                let st = stored_tree_from_opt(archive, backup)?;
                show::show_index_json(st.band(), &mut stdout)?;
            }
            Command::Debug(Debug::Referenced { archive }) => {
                let mut bw = BufWriter::new(stdout);
                let archive = Archive::open(Transport2::new(archive)?)?;
                for hash in archive.referenced_blocks(&archive.list_band_ids()?, monitor)? {
                    writeln!(bw, "{hash}")?;
                }
            }
            Command::Debug(Debug::Unreferenced { archive }) => {
                print!(
                    "{}",
                    Archive::open(Transport2::new(archive)?)?
                        .unreferenced_blocks(monitor)?
                        .map(|hash| format!("{}\n", hash))
                        .collect::<Vec<String>>()
                        .join("")
                );
            }
            Command::Delete {
                archive,
                backup,
                dry_run,
                break_lock,
                no_stats,
            } => {
                let stats = Archive::open(Transport2::new(archive)?)?.delete_bands(
                    backup,
                    &DeleteOptions {
                        dry_run: *dry_run,
                        break_lock: *break_lock,
                    },
                    monitor.clone(),
                )?;
                if !no_stats {
                    monitor.clear_progress_bars();
                    println!("{stats}");
                }
            }
            Command::Diff {
                archive,
                source,
                backup,
                exclude,
                exclude_from,
                include_unchanged,
                json,
            } => {
                let st = stored_tree_from_opt(archive, backup)?;
                let lt = LiveTree::open(source)?;
                let options = DiffOptions {
                    exclude: Exclude::from_patterns_and_files(exclude, exclude_from)?,
                    include_unchanged: *include_unchanged,
                };
                let mut bw = BufWriter::new(stdout);
                for change in diff(&st, &lt, &options, monitor.clone())? {
                    if *json {
                        serde_json::to_writer(&mut bw, &change)?;
                    } else {
                        writeln!(bw, "{change}")?;
                    }
                }
            }
            Command::Gc {
                archive,
                dry_run,
                break_lock,
                no_stats,
            } => {
                let archive = Archive::open(Transport2::new(archive)?)?;
                let stats = archive.delete_bands(
                    &[],
                    &DeleteOptions {
                        dry_run: *dry_run,
                        break_lock: *break_lock,
                    },
                    monitor,
                )?;
                if !no_stats {
                    info!(%stats);
                }
            }
            Command::Init { archive } => {
                Archive::create(Transport2::new(archive)?)?;
                debug!("Created new archive in {archive:?}");
            }
            Command::Ls {
                json,
                stos,
                exclude,
                exclude_from,
                long_listing,
            } => {
                let exclude = Exclude::from_patterns_and_files(exclude, exclude_from)?;
                let entry_iter: Box<dyn Iterator<Item = EntryValue>> =
                    if let Some(archive) = &stos.archive {
                        // TODO: Option for subtree.
                        Box::new(
                            stored_tree_from_opt(archive, &stos.backup)?
                                .iter_entries(Apath::root(), exclude, monitor.clone())?
                                .map(|it| it.into()),
                        )
                    } else {
                        Box::new(LiveTree::open(stos.source.clone().unwrap())?.iter_entries(
                            Apath::root(),
                            exclude,
                            monitor.clone(),
                        )?)
                    };
                monitor.clear_progress_bars();
                if *json {
                    for entry in entry_iter {
                        println!("{}", serde_json::ser::to_string(&entry)?);
                    }
                } else {
                    show::show_entry_names(entry_iter, &mut stdout, *long_listing)?;
                }
            }
            Command::Restore {
                archive,
                destination,
                backup,
                changes_json,
                verbose,
                force_overwrite,
                exclude,
                exclude_from,
                only_subtree,
                long_listing,
                no_stats,
            } => {
                let band_selection = band_selection_policy_from_opt(backup);
                let archive = Archive::open(Transport2::new(archive)?)?;
                let _ = no_stats; // accepted but ignored; we never currently print stats
                let options = RestoreOptions {
                    exclude: Exclude::from_patterns_and_files(exclude, exclude_from)?,
                    only_subtree: only_subtree.clone(),
                    band_selection,
                    overwrite: *force_overwrite,
                    change_callback: make_change_callback(
                        *verbose,
                        *long_listing,
                        &changes_json.as_deref(),
                    )?,
                };
                restore(&archive, destination, &options, monitor)?;
                debug!("Restore complete");
            }
            Command::Size {
                stos,
                bytes,
                exclude,
                exclude_from,
            } => {
                let exclude = Exclude::from_patterns_and_files(exclude, exclude_from)?;
                let size = if let Some(archive) = &stos.archive {
                    stored_tree_from_opt(archive, &stos.backup)?
                        .size(exclude, monitor.clone())?
                        .file_bytes
                } else {
                    LiveTree::open(stos.source.as_ref().unwrap())?
                        .size(exclude, monitor.clone())?
                        .file_bytes
                };
                monitor.clear_progress_bars();
                if *bytes {
                    println!("{size}");
                } else {
                    println!("{}", conserve::bytes_to_human_mb(size));
                }
            }
            Command::Validate { archive, quick, .. } => {
                let options = ValidateOptions {
                    skip_block_hashes: *quick,
                };
                Archive::open(Transport2::new(archive)?)?.validate(&options, monitor.clone())?;
                if monitor.error_count() != 0 {
                    warn!("Archive has some problems.");
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
                let timezone = if *utc {
                    None
                } else {
                    Some(*LOCAL_OFFSET.read().unwrap())
                };
                let archive = Archive::open(Transport2::new(archive)?)?;
                let options = ShowVersionsOptions {
                    newest_first: *newest,
                    tree_size: *sizes,
                    timezone,
                    start_time: !*short,
                    backup_duration: !*short,
                };
                conserve::show_versions(&archive, &options, monitor)?;
            }
        }
        Ok(ExitCode::Success)
    }
}

fn stored_tree_from_opt(archive_location: &str, backup: &Option<BandId>) -> Result<StoredTree> {
    let archive = Archive::open(Transport2::new(archive_location)?)?;
    let policy = band_selection_policy_from_opt(backup);
    archive.open_stored_tree(policy)
}

fn band_selection_policy_from_opt(backup: &Option<BandId>) -> BandSelectionPolicy {
    if let Some(band_id) = backup {
        BandSelectionPolicy::Specified(*band_id)
    } else {
        BandSelectionPolicy::Latest
    }
}

fn make_change_callback<'a>(
    print_changes: bool,
    ls_long: bool,
    changes_json: &Option<&Path>,
) -> Result<Option<ChangeCallback<'a>>> {
    if !print_changes && !ls_long && changes_json.is_none() {
        return Ok(None);
    };

    let changes_json_writer = if let Some(path) = changes_json {
        Some(RefCell::new(BufWriter::new(
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)?,
        )))
    } else {
        None
    };
    Ok(Some(Box::new(move |entry_change| {
        if matches!(entry_change.change, Change::Unchanged { .. }) {
            return Ok(());
        }
        if ls_long {
            let change_meta = entry_change.change.primary_metadata();
            println!(
                "{} {} {} {}",
                entry_change.change.sigil(),
                change_meta.unix_mode,
                change_meta.owner,
                entry_change.apath
            );
        } else if print_changes {
            println!("{} {}", entry_change.change.sigil(), entry_change.apath);
        }
        if let Some(w) = &changes_json_writer {
            let mut w = w.borrow_mut();
            writeln!(
                w,
                "{}",
                serde_json::to_string(entry_change).expect("Failed to serialize change")
            )?;
        }
        Ok(())
    })))
}

fn main() -> Result<ExitCode> {
    // Before anything else, get the local time offset, to avoid `time-rs`
    // problems with loading it when threads are running.
    *LOCAL_OFFSET.write().unwrap() =
        UtcOffset::current_local_offset().expect("get local time offset");
    let args = Args::parse();
    let start_time = Instant::now();
    let console_level = if args.debug {
        Level::TRACE
    } else {
        Level::INFO
    };
    let monitor = Arc::new(TermUiMonitor::new(!args.no_progress));
    let _flush_tracing = enable_tracing(&monitor, &args.trace_time, console_level, &args.log_json);
    let result = args.command.run(monitor.clone());
    debug!(elapsed = ?start_time.elapsed());
    if let Some(metrics_path) = args.metrics_json {
        serde_json::to_writer_pretty(
            File::options()
                .create(true)
                .truncate(true)
                .write(true)
                .open(metrics_path)?,
            monitor.counters(),
        )?;
    }
    match result {
        Err(err) => {
            error!("{err:#}");
            Ok(ExitCode::Failure)
        }
        Ok(ExitCode::Success) if monitor.error_count() != 0 => Ok(ExitCode::NonFatalErrors),
        Ok(exit_code) => Ok(exit_code),
    }
}

#[test]
fn verify_clap() {
    use clap::CommandFactory;
    Args::command().debug_assert()
}
