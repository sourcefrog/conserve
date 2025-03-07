// Copyright 2015-2025 Martin Pool.

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

use std::fs::{create_dir_all, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fail::fail_point;
use filetime::set_file_handle_times;
#[cfg(unix)]
use filetime::set_symlink_file_times;
use time::OffsetDateTime;
use tracing::{instrument, trace, warn};

use crate::blockdir::BlockDir;
use crate::counters::Counter;
use crate::index::entry::IndexEntry;
use crate::io::{directory_is_empty, ensure_dir_exists};
use crate::monitor::Monitor;
use crate::unix_time::ToFileTime;
use crate::*;

/// Description of how to restore a tree.
// #[derive(Debug)]
pub struct RestoreOptions {
    pub exclude: Exclude,
    /// Restore only this subdirectory.
    pub only_subtree: Option<Apath>,
    pub overwrite: bool,
    // The band to select, or by default the last complete one.
    pub band_selection: BandSelectionPolicy,

    // Call this callback as each entry is successfully restored.
    pub change_callback: Option<ChangeCallback>,
}

impl Default for RestoreOptions {
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
    monitor: Arc<dyn Monitor>,
) -> Result<()> {
    let st = archive.open_stored_tree(options.band_selection.clone())?;
    ensure_dir_exists(destination)?;
    if !options.overwrite && !directory_is_empty(destination)? {
        return Err(Error::DestinationNotEmpty);
    }
    let task = monitor.start_task("Restore".to_string());
    let block_dir = &archive.block_dir;
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
        monitor.clone(),
    )?;
    let mut deferrals = Vec::new();
    for entry in entry_iter {
        task.set_name(format!("Restore {}", entry.apath));
        let path = destination.join(&entry.apath[1..]);
        match entry.kind() {
            Kind::Dir => {
                monitor.count(Counter::Dirs, 1);
                if *entry.apath() != Apath::root() {
                    if let Err(err) = create_dir(&path) {
                        if err.kind() != io::ErrorKind::AlreadyExists {
                            monitor.error(Error::RestoreDirectory {
                                path: path.clone(),
                                source: err,
                            });
                            continue;
                        }
                    }
                }
                deferrals.push(DirDeferral {
                    path,
                    unix_mode: entry.unix_mode(),
                    mtime: entry.mtime(),
                    owner: entry.owner().clone(),
                })
            }
            Kind::File => {
                monitor.count(Counter::Files, 1);
                if let Err(err) = restore_file(path.clone(), &entry, block_dir, monitor.clone()) {
                    monitor.error(err);
                    continue;
                }
            }
            Kind::Symlink => {
                monitor.count(Counter::Symlinks, 1);
                if let Err(err) = restore_symlink(&path, &entry) {
                    monitor.error(err);
                    continue;
                }
            }
            Kind::Unknown => {
                monitor.error(Error::InvalidMetadata {
                    details: format!("Unknown file kind {:?}", entry.apath()),
                });
            }
        };
        if let Some(cb) = options.change_callback.as_ref() {
            // Since we only restore to empty directories they're all added.
            cb(&EntryChange::added(&entry))?;
        }
    }
    apply_deferrals(&deferrals, monitor.clone())?;
    Ok(())
}

fn create_dir(path: &Path) -> io::Result<()> {
    fail_point!("restore::create-dir", |_| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Simulated failure",
        ))
    });
    // Create all the parents in case we're restoring only a nested subtree.
    create_dir_all(path)
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
    owner: Owner,
}

fn apply_deferrals(deferrals: &[DirDeferral], monitor: Arc<dyn Monitor>) -> Result<()> {
    for DirDeferral {
        path,
        unix_mode,
        mtime,
        owner,
    } in deferrals
    {
        if let Err(source) = owner.set_owner(path) {
            monitor.error(Error::RestoreOwnership {
                path: path.clone(),
                source,
            });
        }
        if let Err(source) = unix_mode.set_permissions(path) {
            monitor.error(Error::RestorePermissions {
                path: path.clone(),
                source,
            });
        }
        if let Err(source) = filetime::set_file_mtime(path, (*mtime).to_file_time()) {
            monitor.error(Error::RestoreModificationTime {
                path: path.clone(),
                source,
            });
        }
    }
    Ok(())
}

/// Copy in the contents of a file from another tree.
#[instrument(skip(source_entry, block_dir, monitor))]
fn restore_file(
    path: PathBuf,
    source_entry: &IndexEntry,
    block_dir: &BlockDir,
    monitor: Arc<dyn Monitor>,
) -> Result<()> {
    let mut out = File::create(&path).map_err(|err| Error::RestoreFile {
        path: path.clone(),
        source: err,
    })?;
    for addr in &source_entry.addrs {
        // TODO: We could combine small parts
        // in memory, and then write them in a single system call. However
        // for the probably common cases of files with one part, or
        // many larger parts, sending everything through a BufWriter is
        // probably a waste.
        let bytes = block_dir
            .read_address(addr, monitor.clone())
            .map_err(|source| Error::RestoreFileBlock {
                apath: source_entry.apath.clone(),
                hash: addr.hash.clone(),
                source: Box::new(source),
            })?;
        out.write_all(&bytes).map_err(|err| Error::RestoreFile {
            path: path.clone(),
            source: err,
        })?;
        monitor.count(Counter::FileBytes, bytes.len());
    }
    out.flush().map_err(|source| Error::RestoreFile {
        path: path.clone(),
        source,
    })?;

    let mtime = Some(source_entry.mtime().to_file_time());
    set_file_handle_times(&out, mtime, mtime).map_err(|source| Error::RestoreModificationTime {
        path: path.clone(),
        source,
    })?;

    // Restore permissions only if there are mode bits stored in the archive
    if let Err(source) = source_entry.unix_mode().set_permissions(&path) {
        monitor.error(Error::RestorePermissions {
            path: path.clone(),
            source,
        });
    }

    // Restore ownership if possible.
    // TODO: Stats and warnings if a user or group is specified in the index but
    // does not exist on the local system.
    if let Err(source) = source_entry.owner().set_owner(&path) {
        monitor.error(Error::RestoreOwnership {
            path: path.clone(),
            source,
        });
    }
    // TODO: Accumulate more stats.
    trace!("Restored file");
    Ok(())
}

#[cfg(unix)]
fn restore_symlink(path: &Path, entry: &IndexEntry) -> Result<()> {
    use std::os::unix::fs as unix_fs;
    if let Some(ref target) = entry.symlink_target() {
        if let Err(source) = unix_fs::symlink(target, path) {
            return Err(Error::RestoreSymlink {
                path: path.to_owned(),
                source,
            });
        }
        if let Err(source) = entry.owner().set_owner(path) {
            return Err(Error::RestoreOwnership {
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
        return Err(Error::InvalidMetadata {
            details: format!("No target in symlink entry {:?}", entry.apath()),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
#[mutants::skip]
fn restore_symlink(_restore_path: &Path, entry: &IndexEntry) -> Result<()> {
    // TODO: Add a test with a canned index containing a symlink, and expect
    // it cannot be restored on Windows and can be on Unix.
    tracing::warn!("Can't restore symlinks on non-Unix: {}", entry.apath());
    Ok(())
}
