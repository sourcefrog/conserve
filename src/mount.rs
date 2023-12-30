use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{btree_map::Entry, BTreeMap},
    ffi::OsStr,
    fs,
    io::{self, ErrorKind, Read},
    iter::Peekable,
    path::{Component, Path},
    sync::{Arc, Mutex},
};
use tracing::info;

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
    Apath, Archive, BandId, BandSelectionPolicy, Exclude, IndexEntry, IndexRead, Kind, Result,
    StoredTree,
};

macro_rules! static_dir {
    ($name:expr) => {
        DirectoryInfo {
            name: ($name).to_string(),
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

#[derive(Debug)]
struct HunkMetaInfo {
    index: u32,

    start_path: Apath,
    end_path: Apath,
}

struct HunkHelper {
    hunks: Vec<HunkMetaInfo>,
}

impl HunkHelper {
    pub fn from_index(index: &IndexRead) -> Result<Self> {
        let mut hunk_info = index
            .hunks_available()?
            .into_par_iter()
            .map(move |hunk_index| {
                let mut index = index.duplicate();
                let entries = index.read_hunk(hunk_index)?;
                let meta_info = if let Some(entries) = entries {
                    if let (Some(first), Some(last)) = (entries.first(), entries.last()) {
                        Some(HunkMetaInfo {
                            index: hunk_index,

                            start_path: first.apath.clone(),
                            end_path: last.apath.clone(),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(meta_info)
            })
            .map(Result::ok)
            .flatten()
            .filter_map(|entry| entry)
            .collect::<Vec<_>>();

        /* After parallel execution bring all hunks back into order */
        hunk_info.sort_by_key(|info| info.index);
        Ok(Self { hunks: hunk_info })
    }

    pub fn find_hunk_for_file(&self, path: &Apath) -> Option<u32> {
        let hunk_index = self.hunks.binary_search_by(|entry| {
            match (entry.start_path.cmp(&path), entry.end_path.cmp(&path)) {
                (Ordering::Less, Ordering::Less) => Ordering::Less,
                (Ordering::Greater, Ordering::Greater) => Ordering::Greater,
                _ => Ordering::Equal,
            }
        });

        let hunk_index = match hunk_index {
            Ok(index) => index,
            Err(index) => index,
        };

        if hunk_index >= self.hunks.len() {
            None
        } else {
            Some(self.hunks[hunk_index].index)
        }
    }

    pub fn find_hunks_for_subdir(&self, path: &Apath, recursive: bool) -> Vec<u32> {
        /*
         * Appending an empty string to the path allows us to search for the first file
         * in the target directory. This is needed as a file and a directory with the same name are not
         * stored in succession.
         *
         * Example (from the apath test):
         *  - /b/a
         *  - /b/b
         *  - /b/c
         *  - /b/a/c
         *  - /b/b/c
         */
        let search_path = path.append("");
        let directory_start_hunk = match self.hunks.binary_search_by(|entry| {
            match (
                entry.start_path.cmp(&search_path),
                entry.end_path.cmp(&search_path),
            ) {
                (Ordering::Less, Ordering::Less) => Ordering::Less,
                (Ordering::Greater, Ordering::Greater) => Ordering::Greater,
                _ => Ordering::Equal,
            }
        }) {
            Ok(hunk) => hunk,
            Err(hunk) => hunk,
        };

        if directory_start_hunk >= self.hunks.len() {
            return vec![];
        }

        let mut result = Vec::new();
        result.push(self.hunks[directory_start_hunk].index);
        for hunk in &self.hunks[directory_start_hunk + 1..] {
            if !path.is_prefix_of(&hunk.start_path) {
                break;
            }

            if !recursive {
                if hunk.start_path[path.len() + 1..].contains("/") {
                    /* hunk does already contain directory content */
                    break;
                }
            }

            /* hunk still contains subtree elements of that path */
            result.push(hunk.index);
        }

        result
    }
}

struct StoredFileReader {
    iter: Peekable<Box<dyn Iterator<Item = Result<Bytes>>>>,
}

impl StoredFileReader {
    pub fn new(
        stored_tree: Arc<StoredTree>,
        entry: IndexEntry,
        byte_offset: u64,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Self> {
        let file_content = entry
            .addrs
            .into_iter()
            .scan(byte_offset, |skip_bytes, mut entry| {
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
            .map::<Result<Bytes>, _>(move |entry| {
                let content = stored_tree
                    .block_dir()
                    .get_block_content(&entry.hash, monitor.clone())?;

                Ok(content.slice((entry.start as usize)..(entry.start + entry.len) as usize))
            });

        Ok(Self {
            iter: (Box::new(file_content) as Box<dyn Iterator<Item = Result<Bytes>>>).peekable(),
        })
    }
}

impl Read for StoredFileReader {
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

struct ProjectionCache {
    archive: Archive,
    hunks: BTreeMap<BandId, HunkHelper>,
    trees: BTreeMap<BandId, Arc<StoredTree>>,
}

impl ProjectionCache {
    pub fn get_or_open_tree(&mut self, policy: BandSelectionPolicy) -> Result<&Arc<StoredTree>> {
        let band_id = self.archive.resolve_band_id(policy)?;
        match self.trees.entry(band_id) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                info!("Opening band {}", band_id);

                let stored_tree = self
                    .archive
                    .open_stored_tree(BandSelectionPolicy::Specified(band_id))?;

                Ok(entry.insert(Arc::new(stored_tree)))
            }
        }
    }

    pub fn get_or_create_helper(&mut self, stored_tree: &StoredTree) -> Result<&HunkHelper> {
        match self.hunks.entry(stored_tree.band().id()) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                info!("Caching files for band {}", stored_tree.band().id());

                let helper = HunkHelper::from_index(&stored_tree.band().index())?;
                Ok(entry.insert(helper))
            }
        }
    }
}

struct ArchiveProjectionSource {
    archive: Archive,
    cache: Mutex<ProjectionCache>,
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
                        .map(|band_id| static_dir!(format!("{}", band_id)))
                        .collect();

                    return Ok(entries);
                }
            }
            _ => return Ok(vec![]),
        };

        let target_path = components.fold(Apath::root(), |path, component| path.append(&component));
        let (stored_tree, dir_hunks) = {
            let mut cache = self.cache.lock().unwrap();
            let stored_tree = cache.get_or_open_tree(target_band)?.clone();
            let hunk_helper = cache.get_or_create_helper(&stored_tree)?;
            let dir_hunks = hunk_helper.find_hunks_for_subdir(&target_path, false);

            (stored_tree, dir_hunks)
        };

        let tree_index = stored_tree.band().index();
        let iterator = IndexEntryIter::new(
            tree_index.iter_hunks(dir_hunks.into_iter()),
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

        let target_path = components.fold(Apath::root(), |path, component| path.append(&component));
        let (stored_tree, file_hunk) = {
            let mut cache = self.cache.lock().unwrap();
            let stored_tree = cache
                .get_or_open_tree(target_band)
                .map_err(io::Error::other)?
                .clone();

            let hunk_helper = cache
                .get_or_create_helper(&stored_tree)
                .map_err(io::Error::other)?;

            let file_hunk = hunk_helper
                .find_hunk_for_file(&target_path)
                .ok_or(io::Error::new(ErrorKind::NotFound, "invalid path"))?;

            (stored_tree, file_hunk)
        };

        let index_entry = stored_tree
            .band()
            .index()
            .read_hunk(file_hunk)
            .map_err(io::Error::other)?
            .unwrap_or_default()
            .into_iter()
            .find(|entry| entry.apath == target_path)
            .ok_or(io::Error::new(ErrorKind::NotFound, "invalid path"))?;

        let file_size: u64 = index_entry.addrs.iter().map(|addr| addr.len).sum();

        info!(
            "Serving {}/{} ({}/{} bytes)",
            stored_tree.band().id(),
            target_path,
            length,
            file_size
        );
        let reader = StoredFileReader::new(
            stored_tree,
            index_entry,
            byte_offset as u64,
            Arc::new(VoidMonitor),
        )
        .map_err(io::Error::other)?;
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

    let source = ArchiveProjectionSource {
        archive: archive.clone(),
        cache: Mutex::new(ProjectionCache {
            archive,

            hunks: Default::default(),
            trees: Default::default(),
        }),
    };
    let _projection = ProjectedFileSystem::new(destination, source)?;

    info!("Projection started at {}.", destination.display());
    {
        println!("Press any key to stop the projection...");
        let mut stdin = io::stdin();
        let _ = stdin.read(&mut [0u8]).unwrap();
    }

    info!("Stopping projection.");
    if clean {
        debug!("Removing destination {}", destination.display());
        if let Err(err) = fs::remove_dir_all(destination) {
            warn!("Failed to clean up projection destination: {}", err);
        }
    }

    Ok(())
}
