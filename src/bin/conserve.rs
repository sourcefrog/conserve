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
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{Parser, Subcommand};
use conserve::progress::ProgressImpl;
use conserve::trace_counter::{global_error_count, global_warn_count};
use metrics::increment_counter;
#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn, Level};

use conserve::backup::BackupOptions;
use conserve::ui::termui::TraceTimeStyle;
use conserve::ReadTree;
use conserve::RestoreOptions;
use conserve::*;

#[derive(Debug, Parser)]
#[command(author, about, version)]
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

    /// Write metrics to this file.
    #[arg(long, global = true)]
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
        archive: String,
        source: PathBuf,
        #[arg(long, short)]
        backup: Option<BandId>,
        #[arg(long, short)]
        exclude: Vec<String>,
        #[arg(long, short = 'E')]
        exclude_from: Vec<String>,
        #[arg(long)]
        include_unchanged: bool,
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
    fn run(&self) -> Result<ExitCode> {
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
                let source = &LiveTree::open(source)?;
                let options = BackupOptions {
                    exclude: Exclude::from_patterns_and_files(exclude, exclude_from)?,
                    after_entry: make_change_callback(
                        *verbose,
                        *long_listing,
                        &changes_json.as_deref(),
                    )?,
                    ..Default::default()
                };
                if *long_listing || *verbose {
                    // TODO(CON-23): Really Nutmeg should coordinate stdout and stderr...
                    ProgressImpl::Null.activate()
                }
                let stats = backup(&Archive::open(open_transport(archive)?)?, source, &options)?;
                if !no_stats {
                    info!("Backup complete.\n{stats}");
                }
            }
            Command::Debug(Debug::Blocks { archive }) => {
                let mut bw = BufWriter::new(stdout);
                for hash in Archive::open(open_transport(archive)?)?
                    .block_dir()
                    .block_names()?
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
                let archive = Archive::open(open_transport(archive)?)?;
                for hash in archive.referenced_blocks(&archive.list_band_ids()?)? {
                    writeln!(bw, "{hash}")?;
                }
            }
            Command::Debug(Debug::Unreferenced { archive }) => {
                let mut bw = BufWriter::new(stdout);
                for hash in Archive::open(open_transport(archive)?)?.unreferenced_blocks()? {
                    writeln!(bw, "{hash}")?;
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
            } => {
                let st = stored_tree_from_opt(archive, backup)?;
                let lt = LiveTree::open(source)?;
                let options = DiffOptions {
                    exclude: Exclude::from_patterns_and_files(exclude, exclude_from)?,
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
                let archive = Archive::open(open_transport(archive)?)?;
                let stats = archive.delete_bands(
                    &[],
                    &DeleteOptions {
                        dry_run: *dry_run,
                        break_lock: *break_lock,
                    },
                )?;
                if !no_stats {
                    info!(%stats);
                }
            }
            Command::Init { archive } => {
                Archive::create(open_transport(archive)?)?;
                debug!("Created new archive in {archive:?}");
            }
            Command::Ls {
                stos,
                exclude,
                exclude_from,
                long_listing,
            } => {
                let exclude = Exclude::from_patterns_and_files(exclude, exclude_from)?;
                if let Some(archive) = &stos.archive {
                    // TODO: Option for subtree.
                    show::show_entry_names(
                        stored_tree_from_opt(archive, &stos.backup)?
                            .iter_entries(Apath::root(), exclude)?,
                        &mut stdout,
                        *long_listing,
                    )?;
                } else {
                    show::show_entry_names(
                        LiveTree::open(stos.source.clone().unwrap())?
                            .iter_entries(Apath::root(), exclude)?,
                        &mut stdout,
                        *long_listing,
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
                long_listing,
            } => {
                let band_selection = band_selection_policy_from_opt(backup);
                let archive = Archive::open(open_transport(archive)?)?;
                let mut options = RestoreOptions {
                    exclude: Exclude::from_patterns_and_files(exclude, exclude_from)?,
                    only_subtree: only_subtree.clone(),
                    band_selection,
                    overwrite: *force_overwrite,
                    after_entry: None,
                };
                if *long_listing {
                    options.after_entry = Some(Box::new(print_entry_long))
                } else if *verbose {
                    options.after_entry = Some(Box::new(print_entry_short))
                };
                if *verbose || *long_listing {
                    ProgressImpl::Null.activate();
                }
                let stats = restore(&archive, destination, &options)?;
                debug!("Restore complete");
                if !no_stats {
                    debug!(%stats);
                }
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
                        .size(exclude)?
                        .file_bytes
                } else {
                    LiveTree::open(stos.source.as_ref().unwrap())?
                        .size(exclude)?
                        .file_bytes
                };
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
                Archive::open(open_transport(archive)?)?.validate(&options)?;
                if global_error_count() > 0 || global_warn_count() > 0 {
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
                let archive = Archive::open(open_transport(archive)?)?;
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
        Ok(ExitCode::Success)
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

fn print_entry_long(entry: &IndexEntry) {
    println!("{} {} {}", entry.unix_mode(), entry.owner(), entry.apath());
}

fn print_entry_short(entry: &IndexEntry) {
    println!("{}", entry.apath());
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
    Ok(Some(Box::new(move |change| {
        if change.diff_kind == DiffKind::Unchanged {
            return Ok(());
        }
        if ls_long {
            println!(
                "{} {} {} {}",
                change.diff_kind.as_sigil(),
                change.unix_mode,
                change.owner,
                change.apath
            );
        } else if print_changes {
            println!("{} {}", change.diff_kind.as_sigil(), change.apath);
        }
        if let Some(w) = &changes_json_writer {
            let mut w = w.borrow_mut();
            writeln!(
                w,
                "{}",
                serde_json::to_string(change).expect("Failed to serialize change")
            )?;
        }
        Ok(())
    })))
}

fn main() -> Result<ExitCode> {
    let args = Args::parse();
    let start_time = Instant::now();
    if !args.no_progress {
        progress::ProgressImpl::Terminal.activate();
    }
    let trace_level = if args.debug {
        Level::TRACE
    } else {
        Level::INFO
    };
    let _flush_guard = ui::termui::enable_tracing(&args.trace_time, trace_level, &args.log_json);
    ::metrics::set_recorder(&conserve::metric_recorder::IN_MEMORY)
        .expect("Failed to install recorder");
    increment_counter!("conserve.start");
    let result = args.command.run();
    metric_recorder::emit_to_trace();
    debug!(elapsed = ?start_time.elapsed());
    let error_count = global_error_count();
    let warn_count = global_warn_count();
    if let Some(metrics_json_path) = args.metrics_json {
        metric_recorder::write_json_metrics(&metrics_json_path)?;
    }
    match result {
        Err(err) => {
            error!("{err}");
            let mut err: &dyn Error = &err;
            while let Some(source) = err.source() {
                error!("caused by: {source}");
                err = source;
            }
            debug!(error_count, warn_count,);
            Ok(ExitCode::Failure)
        }
        Ok(ExitCode::Success) if error_count > 0 || warn_count > 0 => {
            debug!(error_count, warn_count,);
            Ok(ExitCode::NonFatalErrors)
        }
        Ok(exit_code) => Ok(exit_code),
    }
}

#[test]
fn verify_clap() {
    use clap::CommandFactory;
    Args::command().debug_assert()
}
