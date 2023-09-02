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

//! Access a "live" on-disk tree as a source for backups, destination for restores, etc.

use std::collections::vec_deque::VecDeque;
use std::fs;
use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use tracing::{error, warn};

use crate::entry::{EntryValue, KindMeta};
use crate::owner::Owner;
use crate::stats::LiveTreeIterStats;
use crate::unix_mode::UnixMode;
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

    /// Open a file inside the tree to read.
    pub fn open_file(&self, entry: &EntryValue) -> Result<File> {
        assert_eq!(entry.kind(), Kind::File);
        let path = self.relative_path(&entry.apath);
        fs::File::open(&path).map_err(|source| Error::ReadSourceFile { path, source })
    }
}

impl tree::ReadTree for LiveTree {
    type Entry = EntryValue;
    type IT = Iter;

    fn iter_entries(&self, subtree: Apath, exclude: Exclude) -> Result<Self::IT> {
        Iter::new(&self.path, subtree, exclude)
    }
}

fn entry_from_fs_metadata(
    apath: Apath,
    source_path: &Path,
    metadata: &fs::Metadata,
) -> Result<EntryValue> {
    let mtime = metadata
        .modified()
        .expect("Failed to get file mtime")
        .into();
    let kind_meta = if metadata.is_file() {
        KindMeta::File {
            size: metadata.len(),
        }
    } else if metadata.is_dir() {
        KindMeta::Dir
    } else if metadata.is_symlink() {
        let t = match source_path.read_link() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to read target of symlink {source_path:?}: {e}");
                return Err(e.into());
            }
        };
        let target = match t.into_os_string().into_string() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to decode target of symlink {source_path:?}: {e:?}");
                return Err(Error::UnsupportedTargetEncoding {
                    path: source_path.to_owned(),
                });
            }
        };
        KindMeta::Symlink { target }
    } else {
        return Err(Error::UnsupportedSourceKind {
            path: source_path.to_owned(),
        });
    };
    let owner = Owner::from(metadata);
    let unix_mode = UnixMode::from(metadata.permissions());
    Ok(EntryValue {
        apath,
        mtime,
        kind_meta,
        unix_mode,
        owner,
    })
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
    entry_deque: VecDeque<EntryValue>,

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
        let start_path = subtree.below(root_path);
        let start_metadata = fs::symlink_metadata(&start_path)?;
        // Preload iter to return the root and then recurse into it.
        let entry_deque: VecDeque<EntryValue> = [entry_from_fs_metadata(
            subtree.clone(),
            &start_path,
            &start_metadata,
        )?]
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
        let mut children = Vec::<(String, EntryValue)>::new();
        let dir_path = parent_apath.below(&self.root_path);
        let dir_iter = match fs::read_dir(&dir_path) {
            Ok(i) => i,
            Err(err) => {
                error!("Error reading directory {dir_path:?}: {err}");
                return;
            }
        };
        let mut subdir_apaths: Vec<Apath> = Vec::new();
        for dir_entry in dir_iter {
            let dir_entry = match dir_entry {
                Ok(dir_entry) => dir_entry,
                Err(err) => {
                    error!("Error reading next entry from directory {dir_path:?}: {err}");
                    continue;
                }
            };
            let child_osstr = dir_entry.file_name();
            let child_name = match child_osstr.to_str() {
                Some(c) => c,
                None => {
                    error!("Couldn't decode filename {child_osstr:?} in {dir_path:?}",);
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
                    error!("Error getting type of {child_apath:?} during iteration: {e}");
                    continue;
                }
            };
            if ft.is_dir() {
                // TODO: Count them?
                // TODO: Perhaps an option to back them up anyhow?
                match cachedir::is_tagged(dir_entry.path()) {
                    Ok(true) => continue,
                    Ok(false) => (),
                    Err(e) => {
                        error!("Error checking CACHEDIR.TAG in {dir_entry:?}: {e}");
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
                            warn!("File disappeared during iteration: {child_apath:?}: {e}");
                        }
                        _ => {
                            error!("Failed to read source metadata from {child_apath:?}: {e}");
                            self.stats.metadata_error += 1;
                        }
                    };
                    continue;
                }
            };

            if ft.is_dir() {
                subdir_apaths.push(child_apath.clone());
            }
            let child_path = dir_path.join(dir_entry.file_name());
            let entry = match entry_from_fs_metadata(child_apath, &child_path, &metadata) {
                Ok(entry) => entry,
                Err(Error::UnsupportedSourceKind { .. }) => {
                    // It's not too surprising that there would be fifos or sockets or files
                    // we don't support; don't log them.
                    continue;
                }
                Err(err) => {
                    error!("Failed to build entry for {child_path:?}: {err:?}");
                    continue;
                }
            };
            children.push((child_name.to_string(), entry));
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
    type Item = EntryValue;

    fn next(&mut self) -> Option<EntryValue> {
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
