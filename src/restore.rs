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

//! Restore from the archive to the filesystem.

use std::fs::File;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, time::Instant};

use filetime::set_file_handle_times;
#[cfg(unix)]
use filetime::set_symlink_file_times;
#[cfg(unix)]
use nix::unistd;

use crate::band::BandSelectionPolicy;
use crate::entry::Entry;
use crate::io::{directory_is_empty, ensure_dir_exists};
use crate::stats::RestoreStats;
use crate::unix_mode::UnixMode;
use crate::unix_time::UnixTime;
use crate::*;

/// Description of how to restore a tree.
#[derive(Debug)]
pub struct RestoreOptions {
    pub print_filenames: bool,
    pub exclude: Exclude,
    /// Restore only this subdirectory.
    pub only_subtree: Option<Apath>,
    pub overwrite: bool,
    // The band to select, or by default the last complete one.
    pub band_selection: BandSelectionPolicy,
    /// If printing filenames, include metadata such as file permissions
    pub long_listing: bool,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        RestoreOptions {
            print_filenames: false,
            overwrite: false,
            band_selection: BandSelectionPolicy::LatestClosed,
            exclude: Exclude::nothing(),
            only_subtree: None,
            long_listing: false,
        }
    }
}

struct ProgressModel {
    filename: String,
    bytes_done: u64,
}

impl nutmeg::Model for ProgressModel {
    fn render(&mut self, _width: usize) -> String {
        format!(
            "Restoring: {} MB\n{}",
            self.bytes_done / 1_000_000,
            self.filename
        )
    }
}

/// Restore a selected version, or by default the latest, to a destination directory.
pub fn restore(
    archive: &Archive,
    destination_path: &Path,
    options: &RestoreOptions,
) -> Result<RestoreStats> {
    let st = archive.open_stored_tree(options.band_selection.clone())?;
    let mut rt = if options.overwrite {
        RestoreTree::create_overwrite(destination_path)
    } else {
        RestoreTree::create(destination_path)
    }?;
    let mut stats = RestoreStats::default();
    let progress_bar = nutmeg::View::new(
        ProgressModel {
            filename: String::new(),
            bytes_done: 0,
        },
        ui::nutmeg_options(),
    );
    let start = Instant::now();
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
    for entry in entry_iter {
        if options.print_filenames {
            if options.long_listing {
                progress_bar.message(&format!("{} {}\n", entry.umode(), entry.apath()));
            } else {
                progress_bar.message(&format!("{}\n", entry.apath()));
            }
        }
        progress_bar.update(|model| model.filename = entry.apath().to_string());
        if let Err(e) = match entry.kind() {
            Kind::Dir => {
                stats.directories += 1;
                rt.copy_dir(&entry)
            }
            Kind::File => {
                stats.files += 1;
                let result = rt.copy_file(&entry, &st).map(|s| stats += s);
                if let Some(bytes) = entry.size() {
                    progress_bar.update(|model| model.bytes_done += bytes);
                }
                result
            }
            Kind::Symlink => {
                stats.symlinks += 1;
                rt.copy_symlink(&entry)
            }
            Kind::Unknown => {
                stats.unknown_kind += 1;
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                continue;
            }
        } {
            ui::show_error(&e);
            stats.errors += 1;
            continue;
        }
    }
    stats += rt.finish()?;
    stats.elapsed = start.elapsed();
    // TODO: Merge in stats from the tree iter and maybe the source tree?
    Ok(stats)
}

/// A write-only tree on the filesystem, as a restore destination.
#[derive(Debug)]
pub struct RestoreTree {
    path: PathBuf,

    dir_umodes: Vec<(PathBuf, UnixMode)>,
    dir_mtimes: Vec<(PathBuf, UnixTime)>,
}

impl RestoreTree {
    fn new(path: PathBuf) -> RestoreTree {
        RestoreTree {
            path,
            dir_mtimes: Vec::new(),
            dir_umodes: Vec::new(),
        }
    }

    /// Create a RestoreTree.
    ///
    /// The destination must either not yet exist, or be an empty directory.
    pub fn create<P: Into<PathBuf>>(path: P) -> Result<RestoreTree> {
        let path = path.into();
        match ensure_dir_exists(&path).and_then(|()| directory_is_empty(&path)) {
            Err(source) => Err(Error::Restore { path, source }),
            Ok(true) => Ok(RestoreTree::new(path)),
            Ok(false) => Err(Error::DestinationNotEmpty { path }),
        }
    }

    /// Create a RestoreTree, even if the destination directory is not empty.
    pub fn create_overwrite(path: &Path) -> Result<RestoreTree> {
        Ok(RestoreTree::new(path.to_path_buf()))
    }

    fn rooted_path(&self, apath: &Apath) -> PathBuf {
        // Remove initial slash so that the apath is relative to the destination.
        self.path.join(&apath[1..])
    }

    fn finish(self) -> Result<RestoreStats> {
        #[cfg(unix)]
        for (path, umode) in self.dir_umodes {
            if let Err(err) = fs::set_permissions(path, umode.into()) {
                ui::problem(&format!("Failed to set directory permissions: {:?}", err));
            }
        }
        for (path, time) in self.dir_mtimes {
            if let Err(err) = filetime::set_file_mtime(path, time.into()) {
                ui::problem(&format!("Failed to set directory mtime: {:?}", err));
            }
        }
        Ok(RestoreStats::default())
    }

    fn copy_dir<E: Entry>(&mut self, entry: &E) -> Result<()> {
        let path = self.rooted_path(entry.apath());
        if let Err(source) = fs::create_dir_all(&path) {
            if source.kind() != io::ErrorKind::AlreadyExists {
                return Err(Error::Restore { path, source });
            }
        }
        self.dir_mtimes.push((path.clone(), entry.mtime()));
        self.dir_umodes.push((path, entry.umode()));
        Ok(())
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file<R: ReadTree>(
        &mut self,
        source_entry: &R::Entry,
        from_tree: &R,
    ) -> Result<RestoreStats> {
        let path = self.rooted_path(source_entry.apath());
        let restore_err = |source| Error::Restore {
            path: path.clone(),
            source,
        };
        let mut restore_file = File::create(&path).map_err(restore_err)?;
        // TODO: Read one block at a time: don't pull all the contents into memory.
        let content = &mut from_tree.file_contents(source_entry)?;
        let bytes_copied = std::io::copy(content, &mut restore_file).map_err(restore_err)?;
        restore_file.flush().map_err(restore_err)?;

        let mtime = Some(source_entry.mtime().into());
        set_file_handle_times(&restore_file, mtime, mtime).map_err(|source| {
            Error::RestoreModificationTime {
                path: path.clone(),
                source,
            }
        })?;
        #[cfg(unix)]
        {
            // Restore permissions
            let umode = source_entry.umode();
            fs::set_permissions(&path, umode.into())?;
            // Restore ownership
            let owner = source_entry.owner();
            let uid = if let Some(username) = owner.user {
                users::get_user_by_name(&username).map(|user| user.uid())
            } else {
                None
            };
            let gid = if let Some(groupname) = owner.group {
                users::get_group_by_name(&groupname).map(|group| group.gid())
            } else {
                None
            };
            // TODO: use `std::os::unix::fs::chown(path, uid, gid)?;` once stable
            unistd::chown(
                &path,
                uid.map(unistd::Uid::from_raw),
                gid.map(unistd::Gid::from_raw),
            )
            .map_err(|err| std::io::Error::from(err))?;
        }

        // TODO: Accumulate more stats.
        Ok(RestoreStats {
            uncompressed_file_bytes: bytes_copied,
            ..RestoreStats::default()
        })
    }

    #[cfg(unix)]
    fn copy_symlink<E: Entry>(&mut self, entry: &E) -> Result<()> {
        use std::os::unix::fs as unix_fs;
        if let Some(ref target) = entry.symlink_target() {
            let path = self.rooted_path(entry.apath());
            if let Err(source) = unix_fs::symlink(target, &path) {
                return Err(Error::Restore { path, source });
            }
            let mtime = entry.mtime().into();
            if let Err(source) = set_symlink_file_times(&path, mtime, mtime) {
                return Err(Error::RestoreModificationTime { path, source });
            }
        } else {
            // TODO: Treat as an error.
            ui::problem(&format!("No target in symlink entry {}", entry.apath()));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn copy_symlink<E: Entry>(&mut self, entry: &E) -> Result<()> {
        // TODO: Add a test with a canned index containing a symlink, and expect
        // it cannot be restored on Windows and can be on Unix.
        ui::problem(&format!(
            "Can't restore symlinks on non-Unix: {}",
            entry.apath()
        ));
        Ok(())
    }
}
