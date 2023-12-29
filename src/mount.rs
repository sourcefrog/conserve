use std::{
    borrow::Cow,
    ffi::OsStr,
    fs,
    io::{self, ErrorKind, Read},
    iter::Peekable,
    path::{Component, Path},
    sync::Arc,
};

use bytes::Bytes;
use itertools::Itertools;
use tracing::{debug, warn};
use windows_projfs::{
    DirectoryEntry, DirectoryInfo, FileInfo, ProjectedFileSystem, ProjectedFileSystemSource,
};

use crate::{
    counters::Counter,
    index::IndexEntryIter,
    monitor::{
        task::{Task, TaskList},
        Monitor, Problem,
    },
    Apath, Archive, BandId, BandSelectionPolicy, Exclude, IndexEntry, Kind, Result,
};

macro_rules! static_dir {
    ($name:literal) => {
        DirectoryInfo {
            name: $name.to_string(),
        }
        .into()
    };
}

struct VoidMonitor;
impl Monitor for VoidMonitor {
    fn count(&self, _counter: Counter, _increment: usize) {}

    fn set_counter(&self, _counter: Counter, _value: usize) {}

    fn problem(&self, _problem: Problem) {}

    fn start_task(&self, name: String) -> Task {
        /*
         * All data related to the target task will be dropped
         * as soon the callee drops the task.
         */
        let mut list = TaskList::default();
        list.start_task(name)
    }
}

impl Into<Option<DirectoryEntry>> for IndexEntry {
    fn into(self) -> Option<DirectoryEntry> {
        let file_name = self.apath.split("/").last()?;
        if self.kind == Kind::Dir {
            Some(
                DirectoryInfo {
                    name: file_name.to_string(),
                }
                .into(),
            )
        } else if self.kind == Kind::File {
            Some(
                FileInfo {
                    file_name: file_name.to_string(),
                    file_size: self.addrs.iter().map(|block| block.len).sum(),
                    ..Default::default()
                }
                .into(),
            )
        } else if self.kind == Kind::Symlink {
            /*
             * Awaiting https://github.com/WolverinDEV/windows-projfs/issues/3 to be resolved
             * before we can implement symlinks.
             */
            None
        } else {
            None
        }
    }
}

struct ArchiveProjectionSource {
    archive: Archive,
}

impl ArchiveProjectionSource {
    fn parse_path_band_policy(
        components: &mut dyn Iterator<Item = Cow<'_, str>>,
    ) -> Option<BandSelectionPolicy> {
        match components.next().as_deref() {
            Some("latest") => Some(BandSelectionPolicy::Latest),
            Some("all") => components
                .next()
                .map(|band_id| band_id.parse::<BandId>().ok())
                .flatten()
                .map(BandSelectionPolicy::Specified),
            _ => None,
        }
    }

    fn serve_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>> {
        let mut components = path
            .components()
            .map(Component::as_os_str)
            .map(OsStr::to_string_lossy);

        let target_band = match components.next().as_deref() {
            None => {
                /* Virtual root, display channel selection */
                return Ok(vec![static_dir!("latest"), static_dir!("all")]);
            }
            Some("latest") => BandSelectionPolicy::Latest,
            Some("all") => {
                if let Some(band_id) = components.next() {
                    BandSelectionPolicy::Specified(band_id.parse::<BandId>()?)
                } else {
                    /* list bands */
                    let entries = self
                        .archive
                        .list_band_ids()?
                        .into_iter()
                        .map(|band_id| {
                            DirectoryEntry::Directory(DirectoryInfo {
                                name: format!("{}", band_id),
                            })
                        })
                        .collect();

                    return Ok(entries);
                }
            }
            _ => return Ok(vec![]),
        };

        let stored_tree = self.archive.open_stored_tree(target_band)?;
        let target_path = components.fold(Apath::root(), |path, component| path.append(&component));
        let tree_index = stored_tree.band().index();

        let iterator = IndexEntryIter::new(
            tree_index.iter_hunks(),
            target_path.clone(),
            Exclude::nothing(),
        );

        let path_prefix = target_path.to_string();
        let entries = iterator
            .filter(|entry| {
                if entry.apath.len() <= path_prefix.len() {
                    /*
                     * Skipping the containing directory entry which is eqal to path_prefix.
                     *
                     * Note:
                     * We're not filtering for entries which are not contained within target_path as the
                     * IndexEntryIter already does this.
                     */
                    return false;
                }

                let file_name = &entry.apath[path_prefix.len()..].trim_start_matches("/");
                if file_name.contains("/") {
                    /* entry is a file which is within a sub-directory */
                    return false;
                }

                true
            })
            .filter_map(IndexEntry::into)
            .collect_vec();

        Ok(entries)
    }
}

struct BytesIteratorReader {
    iter: Peekable<Box<dyn Iterator<Item = Result<Bytes>>>>,
}

impl BytesIteratorReader {
    pub fn new(iter: Box<dyn Iterator<Item = Result<Bytes>>>) -> Self {
        Self {
            iter: iter.peekable(),
        }
    }
}

impl Read for BytesIteratorReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut bytes_written = 0;

        while bytes_written < buf.len() {
            let current_chunk = match self.iter.peek_mut() {
                Some(Ok(value)) => value,
                Some(Err(_)) => {
                    return Err(io::Error::other(self.iter.next().unwrap().unwrap_err()))
                }
                None => break,
            };

            let bytes_pending = (buf.len() - bytes_written).min(current_chunk.len());

            buf[bytes_written..(bytes_written + bytes_pending)]
                .copy_from_slice(&current_chunk[0..bytes_pending]);
            bytes_written += bytes_pending;

            if bytes_pending == current_chunk.len() {
                let _ = self.iter.next();
            } else {
                *current_chunk = current_chunk.slice(bytes_pending..);
            }
        }

        Ok(bytes_written)
    }
}

impl ProjectedFileSystemSource for ArchiveProjectionSource {
    fn list_directory(&self, path: &Path) -> Vec<DirectoryEntry> {
        match self.serve_dir(path) {
            Ok(entries) => entries,
            Err(error) => {
                warn!("Failed to serve {}: {}", path.display(), error);
                vec![]
            }
        }
    }

    fn stream_file_content(
        &self,
        path: &Path,
        byte_offset: usize,
        length: usize,
    ) -> std::io::Result<Box<dyn Read>> {
        let mut components = path
            .components()
            .map(Component::as_os_str)
            .map(OsStr::to_string_lossy);

        let target_band = Self::parse_path_band_policy(&mut components)
            .ok_or(io::Error::new(ErrorKind::NotFound, "invalid path"))?;

        let stored_tree = self
            .archive
            .open_stored_tree(target_band)
            .map_err(io::Error::other)?;

        let target_path = components.fold(Apath::root(), |path, component| path.append(&component));
        let index_entry = stored_tree
            .band()
            .index()
            .iter_entries()
            .find(|entry| entry.apath == target_path)
            .ok_or(io::Error::new(ErrorKind::NotFound, "invalid path"))?;

        let void_monitor = Arc::new(VoidMonitor);

        let file_content = index_entry
            .addrs
            .into_iter()
            .scan(byte_offset as u64, |skip_bytes, mut entry| {
                if *skip_bytes == 0 {
                    Some(entry)
                } else if *skip_bytes < entry.len {
                    entry.len -= *skip_bytes;
                    entry.start += *skip_bytes;
                    *skip_bytes = 0;
                    Some(entry)
                } else {
                    *skip_bytes -= entry.len;
                    None
                }
            })
            .map(move |entry| {
                let content = stored_tree
                    .block_dir()
                    .get_block_content(&entry.hash, void_monitor.clone())?;
                Ok(content.slice((entry.start as usize)..(entry.start + entry.len) as usize))
            });

        let reader = BytesIteratorReader::new(Box::new(file_content));
        Ok(Box::new(reader.take(length as u64)))
    }
}

pub fn mount(archive: Archive, destination: &Path, clean: bool) -> Result<()> {
    if clean {
        if destination.exists() {
            eprintln!("The destination already exists.");
            eprintln!("Please ensure, that the destination does not exists.");
            return Ok(());
        }

        fs::create_dir_all(destination)?;
    } else {
        if !destination.exists() {
            eprintln!("The destination does not exists.");
            eprintln!("Please ensure, that the destination does exist prior mounting.");
            return Ok(());
        }
    }

    let source = ArchiveProjectionSource { archive };
    let _projection = ProjectedFileSystem::new(destination, source)?;

    {
        println!("Press any key to stop the projection...");
        let mut stdin = io::stdin();
        let _ = stdin.read(&mut [0u8]).unwrap();
    }

    if clean {
        debug!("Removing destination {}", destination.display());
        if let Err(err) = fs::remove_dir_all(destination) {
            warn!("Failed to clean up projection destination: {}", err);
        }
    }

    Ok(())
}
