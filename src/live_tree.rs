// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2022 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Access a "live" on-disk tree as a source for backups, destination for restores, etc.

use std::collections::vec_deque::VecDeque;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::error;

use crate::owner::Owner;
use crate::stats::LiveTreeIterStats;
use crate::unix_mode::UnixMode;
use crate::unix_time::UnixTime;
use crate::*;

/// A real tree on the filesystem, for use as a backup source or restore destination.
#[derive(Clone)]
pub struct LiveTree {
    path: PathBuf,
}

impl LiveTree {
    /// Open the live tree rooted at `path`.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<LiveTree> {
        // TODO: Maybe fail here if the root doesn't exist or isn't a directory?
        Ok(LiveTree {
            path: path.as_ref().to_path_buf(),
        })
    }

    fn relative_path(&self, apath: &Apath) -> PathBuf {
        apath.below(&self.path)
    }

    /// Return the root path for this tree.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// An in-memory Entry describing a file/dir/symlink in a live tree.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LiveEntry {
    apath: Apath,
    kind: Kind,
    mtime: UnixTime,
    size: Option<u64>,
    symlink_target: Option<String>,
    unix_mode: UnixMode,
    owner: Owner,
}

impl tree::ReadTree for LiveTree {
    type Entry = LiveEntry;
    type R = std::fs::File;
    type IT = Iter;

    fn iter_entries(&self, subtree: Apath, exclude: Exclude) -> Result<Self::IT> {
        Iter::new(&self.path, subtree, exclude)
    }

    fn file_contents(&self, entry: &LiveEntry) -> Result<Self::R> {
        assert_eq!(entry.kind(), Kind::File);
        let path = self.relative_path(&entry.apath);
        fs::File::open(&path).map_err(|source| Error::ReadSourceFile { path, source })
    }

    fn estimate_count(&self) -> Result<u64> {
        // TODO: This stats the file and builds an entry about them, just to
        // throw it away. We could perhaps change the iter to optionally do
        // less work.
        Ok(self
            .iter_entries(Apath::root(), Exclude::nothing())?
            .count() as u64)
    }
}

impl Entry for LiveEntry {
    fn apath(&self) -> &Apath {
        &self.apath
    }

    fn kind(&self) -> Kind {
        self.kind
    }

    fn mtime(&self) -> UnixTime {
        self.mtime
    }

    fn size(&self) -> Option<u64> {
        self.size
    }

    fn symlink_target(&self) -> &Option<String> {
        &self.symlink_target
    }

    fn unix_mode(&self) -> UnixMode {
        self.unix_mode
    }

    fn owner(&self) -> Owner {
        self.owner.clone()
    }
}

impl LiveEntry {
    fn from_fs_metadata(
        apath: Apath,
        metadata: &fs::Metadata,
        symlink_target: Option<String>,
    ) -> LiveEntry {
        // TODO: Could we read the symlink target here, rather than in the caller?
        let mtime = metadata
            .modified()
            .expect("Failed to get file mtime")
            .into();
        let size = if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        };
        let owner = Owner::from(metadata);
        let unix_mode = UnixMode::from(metadata.permissions());
        LiveEntry {
            apath,
            kind: metadata.file_type().into(),
            mtime,
            symlink_target,
            size,
            unix_mode,
            owner,
        }
    }
}

/// Recursive iterator of the contents of a live tree.
///
/// Iterate source files descending through a source directory.
///
/// Visit the files in a directory before descending into its children, as
/// is the defined order for files stored in an archive.  Within those files and
/// child directories, visit them according to a sorted comparison by their UTF-8
/// name.
#[derive(Debug)]
pub struct Iter {
    /// Root of the source tree.
    root_path: PathBuf,

    /// Directories yet to be visited.
    dir_deque: VecDeque<Apath>,

    /// All entries that have been seen but not yet returned by the iterator, in the order they
    /// should be returned.
    entry_deque: VecDeque<LiveEntry>,

    /// Check that emitted paths are in the right order.
    check_order: apath::DebugCheckOrder,

    /// Patterns to exclude from iteration.
    exclude: Exclude,

    stats: LiveTreeIterStats,
}

impl Iter {
    /// Construct a new iter that will visit everything below this root path,
    /// subject to some exclusions
    fn new(root_path: &Path, subtree: Apath, exclude: Exclude) -> Result<Iter> {
        let start_metadata = fs::symlink_metadata(subtree.below(root_path))?;
        // Preload iter to return the root and then recurse into it.
        let entry_deque: VecDeque<LiveEntry> = [LiveEntry::from_fs_metadata(
            subtree.clone(),
            &start_metadata,
            None,
        )]
        .into();
        // TODO: Consider the case where the root is not actually a directory?
        // Should that be supported?
        let dir_deque: VecDeque<Apath> = [subtree].into();
        Ok(Iter {
            root_path: root_path.to_path_buf(),
            entry_deque,
            dir_deque,
            check_order: apath::DebugCheckOrder::new(),
            exclude,
            stats: LiveTreeIterStats::default(),
        })
    }

    /// Visit the next directory.
    ///
    /// Any errors occurring are logged but not returned; we'll continue to
    /// visit whatever can be read.
    fn visit_next_directory(&mut self, parent_apath: &Apath) {
        self.stats.directories_visited += 1;
        // Tuples of (name, entry) so that we can sort children by name.
        let mut children = Vec::<(String, LiveEntry)>::new();
        let dir_path = parent_apath.below(&self.root_path);
        let dir_iter = match fs::read_dir(&dir_path) {
            Ok(i) => i,
            Err(e) => {
                error!("Error reading directory {:?}: {}", &dir_path, e);
                return;
            }
        };
        let mut subdir_apaths: Vec<Apath> = Vec::new();
        for dir_entry in dir_iter {
            let dir_entry = match dir_entry {
                Ok(dir_entry) => dir_entry,
                Err(e) => {
                    error!(
                        "Error reading next entry from directory {:?}: {}",
                        &dir_path, e
                    );
                    continue;
                }
            };
            let child_osstr = dir_entry.file_name();
            let child_name = match child_osstr.to_str() {
                Some(c) => c,
                None => {
                    error!(
                        "Couldn't decode filename {:?} in {:?}",
                        child_osstr, dir_path,
                    );
                    continue;
                }
            };
            let child_apath = parent_apath.append(child_name);

            if self.exclude.matches(&child_apath) {
                self.stats.exclusions += 1;
                continue;
            }

            let ft = match dir_entry.file_type() {
                Ok(ft) => ft,
                Err(e) => {
                    error!(
                        "Error getting type of {:?} during iteration: {}",
                        child_apath, e
                    );
                    continue;
                }
            };
            if ft.is_dir() {
                // TODO: Count them?
                // TODO: Perhaps an option to back them up anyhow?
                match cachedir::is_tagged(&dir_entry.path()) {
                    Ok(true) => continue,
                    Ok(false) => (),
                    Err(e) => {
                        error!("Error checking CACHEDIR.TAG in {:?}: {}", dir_entry, e);
                    }
                }
            }

            let metadata = match dir_entry.metadata() {
                Ok(metadata) => metadata,
                Err(e) => {
                    match e.kind() {
                        ErrorKind::NotFound => {
                            // Fairly harmless, and maybe not even worth logging. Just a race
                            // between listing the directory and looking at the contents.
                            error!(
                                "File disappeared during iteration: {:?}: {}",
                                child_apath, e
                            );
                        }
                        _ => {
                            error!(
                                "Failed to read source metadata from {:?}: {}",
                                child_apath, e
                            );
                            self.stats.metadata_error += 1;
                        }
                    };
                    continue;
                }
            };

            // TODO: Move this into LiveEntry::from_fs_metadata, once there's a
            // global way for it to complain about errors.
            let target: Option<String> = if ft.is_symlink() {
                let t = match dir_path.join(dir_entry.file_name()).read_link() {
                    Ok(t) => t,
                    Err(e) => {
                        error!("Failed to read target of symlink {:?}: {}", child_apath, e);
                        continue;
                    }
                };
                match t.into_os_string().into_string() {
                    Ok(t) => Some(t),
                    Err(e) => {
                        error!(
                            "Failed to decode target of symlink {:?}: {:?}",
                            child_apath, e
                        );
                        continue;
                    }
                }
            } else {
                None
            };
            if ft.is_dir() {
                subdir_apaths.push(child_apath.clone());
            }
            children.push((
                child_name.to_string(),
                LiveEntry::from_fs_metadata(child_apath, &metadata, target),
            ));
        }
        // To get the right overall tree ordering, any new subdirectories
        // discovered here should be visited together in apath order, but before
        // any previously pending directories. In other words, in reverse order
        // push them onto the front of the dir deque.
        if !subdir_apaths.is_empty() {
            subdir_apaths.sort_unstable();
            self.dir_deque.reserve(subdir_apaths.len());
            for a in subdir_apaths.into_iter().rev() {
                self.dir_deque.push_front(a);
            }
        }
        children.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        self.entry_deque.extend(children.into_iter().map(|x| x.1));
    }
}

// The source iterator yields one path at a time as it walks through the source directories.
//
// It has to read each directory entirely so that it can sort the entries.
// These entries are then returned before visiting any subdirectories.
//
// It also has to manage a stack of directories which might be partially walked.  Those
// subdirectories are then visited, also in sorted order, before returning to
// any higher-level directories.
impl Iterator for Iter {
    type Item = LiveEntry;

    fn next(&mut self) -> Option<LiveEntry> {
        loop {
            if let Some(entry) = self.entry_deque.pop_front() {
                // Have already found some entries, so just return the first.
                self.stats.entries_returned += 1;
                // Sanity check that all the returned paths are in correct order.
                self.check_order.check(&entry.apath);
                return Some(entry);
            } else if let Some(entry) = self.dir_deque.pop_front() {
                // No entries already queued, visit a new directory to try to refill the queue.
                self.visit_next_directory(&entry)
            } else {
                // No entries queued and no more directories to visit.
                return None;
            }
        }
    }
}
