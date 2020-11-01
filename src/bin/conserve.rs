// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

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

use structopt::StructOpt;

use conserve::backup::BackupOptions;
use conserve::ReadTree;
use conserve::RestoreOptions;
use conserve::*;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "conserve",
    about = "A robust backup tool <https://github.com/sourcefrog/conserve/>",
    author
)]
enum Command {
    /// Copy source directory into an archive.
    Backup {
        /// Path of an existing archive.
        archive: PathBuf,
        /// Source directory to copy from.
        source: PathBuf,
        /// Print copied file names.
        #[structopt(long, short)]
        verbose: bool,
        #[structopt(long, short, number_of_values = 1)]
        exclude: Vec<String>,
    },

    Debug(Debug),

    /// Delete backups from an archive.
    Delete {
        /// Archive to delete from.
        archive: PathBuf,
        /// Backup to delete.
        #[structopt(long, short, multiple(true), required(true), number_of_values(1))]
        backup: Vec<BandId>,
        /// Don't actually delete, just check what could be deleted.
        #[structopt(long)]
        dry_run: bool,
        /// Break a lock left behind by a previous interrupted gc operation, and then gc.
        #[structopt(long)]
        break_lock: bool,
        /// Delete indexes but don't garbage-collect blocks.
        ///
        /// (Faster, but doesn't free up space. The archive should later be gc'd.)
        #[structopt(long)]
        no_gc: bool,
    },

    /// Compare a stored tree to a source directory.
    Diff {
        archive: PathBuf,
        source: PathBuf,
        #[structopt(long, short)]
        backup: Option<BandId>,
        #[structopt(long, short, number_of_values = 1)]
        exclude: Vec<String>,
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
        #[structopt(long)]
        dry_run: bool,
        /// Break a lock left behind by a previous interrupted gc operation, and then gc.
        #[structopt(long)]
        break_lock: bool,
    },

    /// List files in a stored tree or source directory, with exclusions.
    Ls {
        #[structopt(flatten)]
        stos: StoredTreeOrSource,

        #[structopt(long, short, number_of_values = 1)]
        exclude: Vec<String>,
    },

    /// Copy a stored tree to a restore directory.
    Restore {
        archive: PathBuf,
        destination: PathBuf,
        #[structopt(long, short)]
        backup: Option<BandId>,
        #[structopt(long, short)]
        force_overwrite: bool,
        #[structopt(long, short)]
        verbose: bool,
        #[structopt(long, short, number_of_values = 1)]
        exclude: Vec<String>,
        #[structopt(long = "only", short = "i", number_of_values = 1)]
        only_subtree: Option<Apath>,
    },

    /// Show the total size of files in a stored tree or source directory, with exclusions.
    Size {
        #[structopt(flatten)]
        stos: StoredTreeOrSource,

        /// Count in bytes, not megabytes.
        #[structopt(long)]
        bytes: bool,

        #[structopt(long, short, number_of_values = 1)]
        exclude: Vec<String>,
    },

    /// Check that an archive is internally consistent.
    Validate {
        /// Path of the archive to check.
        archive: PathBuf,
    },

    /// List backup versions in an archive.
    Versions {
        archive: PathBuf,
        /// Show only version names.
        #[structopt(long, short = "q")]
        short: bool,
        /// Show size of stored trees.
        #[structopt(long, short = "z", conflicts_with = "short")]
        sizes: bool,
    },
}

#[derive(Debug, StructOpt)]
struct StoredTreeOrSource {
    #[structopt(required_unless = "source")]
    archive: Option<PathBuf>,

    /// List files in a source directory rather than an archive.
    #[structopt(long, short, conflicts_with = "archive", required_unless = "archive")]
    source: Option<PathBuf>,

    #[structopt(long, short, conflicts_with = "source")]
    backup: Option<BandId>,
}

/// Show debugging information.
#[derive(Debug, StructOpt)]
enum Debug {
    /// Dump the index as json.
    Index {
        /// Path of the archive to read.
        archive: PathBuf,

        /// Backup version number.
        #[structopt(long, short)]
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
            } => {
                let excludes = excludes::from_strings(exclude)?;
                let source = &LiveTree::open(source)?;
                let options = BackupOptions {
                    print_filenames: *verbose,
                    excludes,
                    ..Default::default()
                };
                let copy_stats = backup(&Archive::open_path(archive)?, &source, &options)?;
                ui::println("Backup complete.");
                copy_stats.summarize_backup(&mut stdout);
            }
            Command::Debug(Debug::Blocks { archive }) => {
                let mut bw = BufWriter::new(stdout);
                for hash in Archive::open_path(archive)?.block_dir().block_names()? {
                    writeln!(bw, "{}", hash)?;
                }
            }
            Command::Debug(Debug::Index { archive, backup }) => {
                let st = stored_tree_from_opt(archive, &backup)?;
                output::show_index_json(&st.band(), &mut stdout)?;
            }
            Command::Debug(Debug::Referenced { archive }) => {
                let mut bw = BufWriter::new(stdout);
                for hash in Archive::open_path(archive)?.referenced_blocks()? {
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
                no_gc,
                break_lock,
            } => {
                let stats = Archive::open_path(archive)?.delete_bands(
                    &backup,
                    &DeleteOptions {
                        dry_run: *dry_run,
                        break_lock: *break_lock,
                        no_gc: *no_gc,
                    },
                )?;
                ui::println(&format!("{:#?}", stats));
            }
            Command::Diff {
                archive,
                source,
                backup,
                exclude,
            } => {
                // TODO: Consider whether the actual files have changed.
                // TODO: Summarize diff.
                // TODO: Optionally include unchanged files.
                let excludes = excludes::from_strings(exclude)?;
                let st = stored_tree_from_opt(archive, backup)?;
                let lt = LiveTree::open(source)?;
                output::show_tree_diff(
                    &mut MergeTrees::new(
                        st.iter_filtered(None, excludes.clone())?,
                        lt.iter_filtered(None, excludes)?,
                    ),
                    &mut stdout,
                )?;
            }
            Command::Gc {
                archive,
                dry_run,
                break_lock,
            } => {
                let archive = Archive::open_path(archive)?;
                let stats = archive.delete_unreferenced(&DeleteOptions {
                    dry_run: *dry_run,
                    break_lock: *break_lock,
                    no_gc: false,
                })?;
                ui::println(&format!("{:#?}", stats));
            }
            Command::Init { archive } => {
                Archive::create_path(&archive)?;
                ui::println(&format!("Created new archive in {:?}", &archive));
            }
            Command::Ls { stos, exclude } => {
                let excludes = excludes::from_strings(exclude)?;
                if let Some(archive) = &stos.archive {
                    output::show_entry_names(
                        stored_tree_from_opt(archive, &stos.backup)?
                            .iter_filtered(None, excludes)?,
                        &mut stdout,
                    )?;
                } else {
                    output::show_entry_names(
                        LiveTree::open(stos.source.clone().unwrap())?
                            .iter_filtered(None, excludes)?,
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
                only_subtree,
            } => {
                let band_selection = band_selection_policy_from_opt(backup);
                let archive = Archive::open_path(archive)?;

                let options = RestoreOptions {
                    print_filenames: *verbose,
                    excludes: excludes::from_strings(exclude)?,
                    only_subtree: only_subtree.clone(),
                    band_selection,
                    overwrite: *force_overwrite,
                };

                let copy_stats = archive.restore(&destination, &options)?;
                ui::println("Restore complete.");
                copy_stats.summarize_restore(&mut stdout)?;
            }
            Command::Size {
                ref stos,
                bytes,
                ref exclude,
            } => {
                let excludes = excludes::from_strings(exclude)?;
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
            Command::Validate { archive } => {
                let stats = Archive::open_path(archive)?.validate()?;
                stats.summarize(&mut stdout)?;
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
                sizes,
            } => {
                ui::enable_progress(false);
                let archive = Archive::open_path(archive)?;
                if *short {
                    output::show_brief_version_list(&archive, &mut stdout)?;
                } else {
                    output::show_verbose_version_list(&archive, *sizes, &mut stdout)?;
                }
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
    ui::enable_progress(true);
    let result = Command::from_args().run();
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
