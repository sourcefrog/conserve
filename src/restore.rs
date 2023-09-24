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

//! Restore from the archive to the filesystem.

use std::fs::File;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, time::Instant};

use filetime::set_file_handle_times;
#[cfg(unix)]
use filetime::set_symlink_file_times;
use time::OffsetDateTime;
use tracing::{error, instrument, warn};

use crate::band::BandSelectionPolicy;
use crate::io::{directory_is_empty, ensure_dir_exists};
use crate::progress::{Bar, Progress};
use crate::stats::RestoreStats;
use crate::unix_mode::UnixMode;
use crate::unix_time::ToFileTime;
use crate::*;

/// Description of how to restore a tree.
// #[derive(Debug)]
pub struct RestoreOptions<'cb> {
    pub exclude: Exclude,
    /// Restore only this subdirectory.
    pub only_subtree: Option<Apath>,
    pub overwrite: bool,
    // The band to select, or by default the last complete one.
    pub band_selection: BandSelectionPolicy,

    // Call this callback as each entry is successfully restored.
    pub change_callback: Option<ChangeCallback<'cb>>,
}

impl Default for RestoreOptions<'_> {
    fn default() -> Self {
        RestoreOptions {
            overwrite: false,
            band_selection: BandSelectionPolicy::LatestClosed,
            exclude: Exclude::nothing(),
            only_subtree: None,
            change_callback: None,
        }
    }
}

/// Restore a selected version, or by default the latest, to a destination directory.
pub fn restore(
    archive: &Archive,
    destination: &Path,
    options: &RestoreOptions,
) -> Result<RestoreStats> {
    let st = archive.open_stored_tree(options.band_selection.clone())?;
    ensure_dir_exists(destination)?;
    if !options.overwrite && !directory_is_empty(destination)? {
        return Err(Error::DestinationNotEmpty);
    }
    let mut stats = RestoreStats::default();
    let mut bytes_done = 0;
    let bar = Bar::new();
    let start = Instant::now();
    let block_dir = archive.block_dir();
    // // This causes us to walk the source tree twice, which is probably an acceptable option
    // // since it's nice to see realistic overall progress. We could keep all the entries
    // // in memory, and maybe we should, but it might get unreasonably big.
    // if options.measure_first {
    //     progress_bar.set_phase("Measure source tree");
    //     // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
    //     // again a second time? But, that'll potentially use memory proportional to tree size, which
    //     // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
    //     // deleted or changed while this is running.
    //     progress_bar.set_bytes_total(st.size(options.excludes.clone())?.file_bytes as u64);
    // }
    let entry_iter = st.iter_entries(
        options.only_subtree.clone().unwrap_or_else(Apath::root),
        options.exclude.clone(),
    )?;
    let mut deferrals = Vec::new();
    for entry in entry_iter {
        bar.post(Progress::Restore {
            filename: entry.apath().to_string(),
            bytes_done,
        });
        let path = destination.join(&entry.apath[1..]);
        match entry.kind() {
            Kind::Dir => {
                stats.directories += 1;
                if let Err(err) = fs::create_dir_all(&path) {
                    if err.kind() != io::ErrorKind::AlreadyExists {
                        error!(?path, ?err, "Failed to create directory");
                        stats.errors += 1;
                        continue;
                    }
                }
                deferrals.push(DirDeferral {
                    path,
                    unix_mode: entry.unix_mode(),
                    mtime: entry.mtime(),
                })
            }
            Kind::File => {
                stats.files += 1;
                match restore_file(path.clone(), &entry, block_dir) {
                    Err(err) => {
                        error!(?err, ?path, "Failed to restore file");
                        stats.errors += 1;
                        continue;
                    }
                    Ok(s) => {
                        if let Some(bytes) = entry.size() {
                            bytes_done += bytes;
                        }
                        stats += s;
                    }
                }
            }
            Kind::Symlink => {
                stats.symlinks += 1;
                if let Err(err) = restore_symlink(&path, &entry) {
                    error!(?path, ?err, "Failed to restore symlink");
                    stats.errors += 1;
                    continue;
                }
            }
            Kind::Unknown => {
                stats.unknown_kind += 1;
                warn!(apath = ?entry.apath(), "Unknown file kind");
            }
        };
        if let Some(cb) = options.change_callback.as_ref() {
            // Since we only restore to empty directories they're all added.
            cb(&EntryChange::added(&entry))?;
        }
    }
    stats += apply_deferrals(&deferrals)?;
    stats.elapsed = start.elapsed();
    // TODO: Merge in stats from the tree iter and maybe the source tree?
    Ok(stats)
}

/// Recorded changes to apply to directories after all their contents
/// have been applied.
///
/// For example we might want to make the directory read-only, but we
/// shouldn't do that until we added all the children.
struct DirDeferral {
    path: PathBuf,
    unix_mode: UnixMode,
    mtime: OffsetDateTime,
}

fn apply_deferrals(deferrals: &[DirDeferral]) -> Result<RestoreStats> {
    let mut stats = RestoreStats::default();
    for DirDeferral {
        path,
        unix_mode,
        mtime,
    } in deferrals
    {
        if let Err(err) = unix_mode.set_permissions(path) {
            error!(?path, ?err, "Failed to set directory permissions");
            stats.errors += 1;
        }
        if let Err(err) = filetime::set_file_mtime(path, (*mtime).to_file_time()) {
            error!(?path, ?err, "Failed to set directory mtime");
            stats.errors += 1;
        }
    }
    Ok(stats)
}

/// Copy in the contents of a file from another tree.
#[instrument(skip(source_entry, block_dir))]
fn restore_file(
    path: PathBuf,
    source_entry: &IndexEntry,
    block_dir: &BlockDir,
) -> Result<RestoreStats> {
    let mut stats = RestoreStats::default();
    let mut out = File::create(&path).map_err(|err| {
        error!(?path, ?err, "Error creating destination file");
        Error::Restore {
            path: path.clone(),
            source: err,
        }
    })?;
    let mut len = 0u64;
    for addr in &source_entry.addrs {
        // TODO: We could combine small parts
        // in memory, and then write them in a single system call. However
        // for the probably common cases of files with one part, or
        // many larger parts, sending everything through a BufWriter is
        // probably a waste.
        let bytes = block_dir.read_address(addr).map_err(|err| {
            error!(?path, ?err, "Failed to read block content for file");
            err
        })?;
        out.write_all(&bytes).map_err(|err| {
            error!(?path, ?err, "Failed to write content to restore file");
            Error::Restore {
                path: path.clone(),
                source: err,
            }
        })?;
        len += bytes.len() as u64;
    }
    stats.uncompressed_file_bytes = len;
    out.flush().map_err(|source| Error::Restore {
        path: path.clone(),
        source,
    })?;

    let mtime = Some(source_entry.mtime().to_file_time());
    set_file_handle_times(&out, mtime, mtime).map_err(|source| Error::RestoreModificationTime {
        path: path.clone(),
        source,
    })?;

    // Restore permissions only if there are mode bits stored in the archive
    if let Err(err) = source_entry.unix_mode().set_permissions(&path) {
        error!(?path, ?err, "Error restoring unix permissions");
        stats.errors += 1;
    }

    // Restore ownership if possible.
    // TODO: Stats and warnings if a user or group is specified in the index but
    // does not exist on the local system.
    if let Err(err) = &source_entry.owner().set_owner(&path) {
        error!(?path, ?err, "Error restoring ownership");
        stats.errors += 1;
    }
    // TODO: Accumulate more stats.
    Ok(stats)
}

#[cfg(unix)]
fn restore_symlink(path: &Path, entry: &IndexEntry) -> Result<()> {
    use std::os::unix::fs as unix_fs;
    if let Some(ref target) = entry.symlink_target() {
        if let Err(source) = unix_fs::symlink(target, path) {
            return Err(Error::Restore {
                path: path.to_owned(),
                source,
            });
        }
        let mtime = entry.mtime().to_file_time();
        if let Err(source) = set_symlink_file_times(path, mtime, mtime) {
            return Err(Error::RestoreModificationTime {
                path: path.to_owned(),
                source,
            });
        }
    } else {
        error!(apath = ?entry.apath(), "No target in symlink entry");
    }
    Ok(())
}

#[cfg(not(unix))]
fn restore_symlink(_restore_path: &Path, entry: &IndexEntry) -> Result<()> {
    // TODO: Add a test with a canned index containing a symlink, and expect
    // it cannot be restored on Windows and can be on Unix.
    warn!("Can't restore symlinks on non-Unix: {}", entry.apath());
    Ok(())
}
