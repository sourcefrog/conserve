use std::{
    borrow::Cow,
    ffi::OsStr,
    fs,
    io::{self, ErrorKind, Read},
    iter::Peekable,
    num::NonZeroUsize,
    ops::ControlFlow,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use bytes::Bytes;
use itertools::Itertools;
use lru::LruCache;
use tracing::{debug, error, info, warn};
use windows_projfs::{
    DirectoryEntry, DirectoryInfo, FileInfo, Notification, ProjectedFileSystem,
    ProjectedFileSystemSource,
};

use crate::{
    hunk_index::IndexHunkIndex,
    monitor::{void::VoidMonitor, Monitor},
    Apath, Archive, BandId, BandSelectionPolicy, IndexEntry, Kind, Result, StoredTree,
};

use super::MountOptions;

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

const UNIX_WIN_DIFF_SECS: i64 = 11644473600;
fn unix_time_to_windows(unix_seconds: i64, unix_nanos: u32) -> u64 {
    if unix_seconds < -UNIX_WIN_DIFF_SECS {
        return 0;
    }

    let win_seconds = (unix_seconds + UNIX_WIN_DIFF_SECS) as u64;
    win_seconds * 1_000_000_000 / 100 + (unix_nanos / 100) as u64
}

/* https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants */
const FILE_ATTRIBUTE_READONLY: u32 = 0x00000001;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x00000010;
const FILE_ATTRIBUTE_NOT_CONTENT_INDEXED: u32 = 0x00002000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x00040000;

/* Note: Using FILE_ATTRIBUTE_READONLY on directories will cause the explorer to *always* list all second level subdirectory entries */
const DIRECTORY_ATTRIBUTES: u32 =
    FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED | FILE_ATTRIBUTE_RECALL_ON_OPEN;

fn index_entry_to_directory_entry(entry: &IndexEntry) -> Option<DirectoryEntry> {
    let file_name = entry.apath.split('/').last()?;
    if entry.kind == Kind::Dir {
        Some(
            DirectoryInfo {
                directory_name: file_name.to_string(),
                directory_attributes: DIRECTORY_ATTRIBUTES,

                /* currently conserve does not differentiate between the different time stamps */
                creation_time: unix_time_to_windows(entry.mtime, entry.mtime_nanos),
                last_access_time: unix_time_to_windows(entry.mtime, entry.mtime_nanos),
                last_write_time: unix_time_to_windows(entry.mtime, entry.mtime_nanos),
            }
            .into(),
        )
    } else if entry.kind == Kind::File {
        Some(
            FileInfo {
                file_name: file_name.to_string(),
                file_size: entry.addrs.iter().map(|block| block.len).sum(),
                file_attributes: FILE_ATTRIBUTE_READONLY,

                /* currently conserve does not differentiate between the different time stamps */
                creation_time: unix_time_to_windows(entry.mtime, entry.mtime_nanos),
                last_access_time: unix_time_to_windows(entry.mtime, entry.mtime_nanos),
                last_write_time: unix_time_to_windows(entry.mtime, entry.mtime_nanos),
            }
            .into(),
        )
    } else {
        None
    }
}

struct ArchiveProjectionSource {
    archive: Archive,

    stored_tree_cache: Mutex<LruCache<BandId, Arc<StoredTree>>>,

    hunk_index_cache: Mutex<LruCache<BandId, Arc<IndexHunkIndex>>>,

    /*
     * Cache the last accessed hunks to improve directory travesal speed.
     */
    #[allow(clippy::type_complexity)]
    hunk_content_cache: Mutex<LruCache<(BandId, u32), Arc<Vec<IndexEntry>>>>,

    /*
     * The Windows file explorer has the tendency to query some directories multiple times in a row.
     * Also if the user navigates up/down, allow this cache to help.
     */
    serve_dir_cache: Mutex<LruCache<PathBuf, Vec<DirectoryEntry>>>,
}

impl ArchiveProjectionSource {
    pub fn load_hunk_contents(
        &self,
        stored_tree: &StoredTree,
        hunk_id: u32,
    ) -> Result<Arc<Vec<IndexEntry>>> {
        let band_id = stored_tree.band().id();
        self.hunk_content_cache
            .lock()
            .unwrap()
            .try_get_or_insert((band_id, hunk_id), || {
                let mut index = stored_tree.band().index();
                Ok(Arc::new(index.read_hunk(hunk_id)?.unwrap_or_default()))
            })
            .cloned()
    }

    pub fn get_or_open_tree(&self, policy: BandSelectionPolicy) -> Result<Arc<StoredTree>> {
        let band_id = self.archive.resolve_band_id(policy)?;
        self.stored_tree_cache
            .lock()
            .unwrap()
            .try_get_or_insert(band_id, || {
                debug!("Opening band {}", band_id);

                let stored_tree = self
                    .archive
                    .open_stored_tree(BandSelectionPolicy::Specified(band_id))?;

                Ok(Arc::new(stored_tree))
            })
            .cloned()
    }

    pub fn get_or_create_hunk_index(
        &self,
        stored_tree: &StoredTree,
    ) -> Result<Arc<IndexHunkIndex>> {
        let band_id = stored_tree.band().id();
        self.hunk_index_cache
            .lock()
            .unwrap()
            .try_get_or_insert(band_id, || {
                /* Inform the user that this band has been cached as this is most likely a heavy operaton (cpu and memory wise) */
                info!("Caching files for band {}", stored_tree.band().id());

                let helper = IndexHunkIndex::from_index(&stored_tree.band().index())?;
                Ok(Arc::new(helper))
            })
            .cloned()
    }

    fn parse_path_band_policy(
        components: &mut dyn Iterator<Item = Cow<'_, str>>,
    ) -> Option<BandSelectionPolicy> {
        match components.next().as_deref() {
            Some("latest") => Some(BandSelectionPolicy::Latest),
            Some("all") => components
                .next()
                .and_then(|band_id| band_id.parse::<BandId>().ok())
                .map(BandSelectionPolicy::Specified),
            _ => None,
        }
    }

    fn band_id_to_directory_info(&self, policy: BandSelectionPolicy) -> Option<DirectoryInfo> {
        let stored_tree = self.get_or_open_tree(policy).ok()?;
        let band_info = stored_tree.band().get_info().ok()?;

        let timestamp = unix_time_to_windows(
            band_info.start_time.unix_timestamp(),
            band_info.start_time.unix_timestamp_nanos() as u32,
        );

        Some(DirectoryInfo {
            directory_name: format!("{}", band_info.id),
            directory_attributes: DIRECTORY_ATTRIBUTES,

            creation_time: timestamp,
            last_access_time: timestamp,
            last_write_time: timestamp,
        })
    }

    fn serve_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>> {
        debug!("Serving directory {}", path.display());

        let mut components = path
            .components()
            .map(Component::as_os_str)
            .map(OsStr::to_string_lossy);

        let target_band = match components.next().as_deref() {
            None => {
                /* Virtual root, display band selection */
                let mut entries = Vec::with_capacity(2);
                entries.push(DirectoryEntry::Directory(DirectoryInfo {
                    directory_name: "all".to_string(),
                    directory_attributes: DIRECTORY_ATTRIBUTES,

                    ..Default::default()
                }));
                if let Some(mut info) = self.band_id_to_directory_info(BandSelectionPolicy::Latest)
                {
                    info.directory_name = "latest".to_string();
                    entries.push(DirectoryEntry::Directory(info))
                }

                return Ok(entries);
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
                        .filter_map(|band_id| {
                            self.band_id_to_directory_info(BandSelectionPolicy::Specified(band_id))
                                .map(DirectoryEntry::Directory)
                        })
                        .collect();

                    return Ok(entries);
                }
            }
            _ => return Ok(vec![]),
        };

        let target_path = components.fold(Apath::root(), |path, component| path.append(&component));
        let stored_tree = self.get_or_open_tree(target_band)?;
        let hunk_index = self.get_or_create_hunk_index(&stored_tree)?;
        let dir_hunks = hunk_index.find_hunks_for_subdir(&target_path, false);

        let hunks = dir_hunks
            .into_iter()
            .flat_map(|hunk_id| self.load_hunk_contents(&stored_tree, hunk_id).ok())
            .collect_vec();

        let iterator = hunks.iter().flat_map(|e| &**e);

        let path_prefix = target_path.to_string();
        let entries = iterator
            .filter(|entry| {
                if !entry.apath.starts_with(&path_prefix) {
                    /* Not the directory we're interested in */
                    return false;
                }

                if entry.apath.len() <= path_prefix.len() {
                    /*
                     * Skipping the containing directory entry which is eqal to path_prefix.
                     */
                    return false;
                }

                let file_name = &entry.apath[path_prefix.len()..].trim_start_matches('/');
                if file_name.contains('/') {
                    /* entry is a file which is within a sub-directory */
                    return false;
                }

                true
            })
            .filter_map(index_entry_to_directory_entry)
            .collect_vec();

        Ok(entries)
    }

    fn serve_file(
        &self,
        path: &Path,
        byte_offset: usize,
        length: usize,
    ) -> io::Result<Box<dyn Read>> {
        let mut components = path
            .components()
            .map(Component::as_os_str)
            .map(OsStr::to_string_lossy);

        let target_band = Self::parse_path_band_policy(&mut components)
            .ok_or(io::Error::new(ErrorKind::NotFound, "invalid path"))?;

        let target_path = components.fold(Apath::root(), |path, component| path.append(&component));
        let stored_tree = self
            .get_or_open_tree(target_band)
            .map_err(io::Error::other)?;

        let hunk_index = self
            .get_or_create_hunk_index(&stored_tree)
            .map_err(io::Error::other)?;
        let file_hunk = hunk_index
            .find_hunk_for_file(&target_path)
            .ok_or(io::Error::new(ErrorKind::NotFound, "invalid path"))?;

        let index_entry = self
            .load_hunk_contents(&stored_tree, file_hunk)
            .map_err(io::Error::other)?
            .iter()
            .find(|entry| entry.apath == target_path)
            .ok_or(io::Error::new(ErrorKind::NotFound, "invalid path"))?
            .clone();

        let file_size: u64 = index_entry.addrs.iter().map(|addr| addr.len).sum();

        debug!(
            "Serving {}{} ({}/{} bytes)",
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

impl ProjectedFileSystemSource for ArchiveProjectionSource {
    fn list_directory(&self, path: &Path) -> Vec<DirectoryEntry> {
        let cached_result = self
            .serve_dir_cache
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(path).cloned());

        if let Some(cached_result) = cached_result {
            return cached_result;
        }

        match self.serve_dir(path) {
            Ok(entries) => {
                self.serve_dir_cache
                    .lock()
                    .unwrap()
                    .push(path.to_owned(), entries.clone());

                entries
            }
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
        match self.serve_file(path, byte_offset, length) {
            Ok(reader) => Ok(reader),
            Err(error) => {
                if error.kind() != ErrorKind::NotFound {
                    warn!("Failed to serve file {}: {}", path.display(), error);
                }

                Err(error)
            }
        }
    }

    fn handle_notification(&self, notification: &Notification) -> ControlFlow<()> {
        if notification.is_cancelable()
            && !matches!(notification, Notification::FilePreConvertToFull(_))
        {
            /* try to cancel everything, except retriving data */
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

const ERROR_CODE_VIRTUALIZATION_TEMPORARILY_UNAVAILABLE: i32 = 369;
pub fn mount(archive: Archive, destination: &Path, options: MountOptions) -> Result<()> {
    if options.clean {
        if destination.exists() {
            error!("The destination already exists.");
            error!("Please ensure, that the destination does not exists.");
            return Ok(());
        }

        fs::create_dir_all(destination)?;
    } else if !destination.exists() {
        error!("The destination does not exists.");
        error!("Please ensure, that the destination does exist prior mounting.");
        return Ok(());
    }

    let source = ArchiveProjectionSource {
        archive: archive.clone(),

        /* cache at most 16 different bands in parallel */
        stored_tree_cache: Mutex::new(LruCache::new(NonZeroUsize::new(16).unwrap())),
        hunk_index_cache: Mutex::new(LruCache::new(NonZeroUsize::new(16).unwrap())),

        hunk_content_cache: Mutex::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
        serve_dir_cache: Mutex::new(LruCache::new(NonZeroUsize::new(32).unwrap())),
    };

    let projection = ProjectedFileSystem::new(destination, source)?;
    info!("Projection started at {}.", destination.display());
    {
        info!("Press any key to stop the projection...");
        let mut stdin = io::stdin();
        let _ = stdin.read(&mut [0u8]).unwrap();
    }

    info!("Stopping projection.");
    drop(projection);

    if options.clean {
        debug!("Removing destination {}", destination.display());
        let mut attempt_count = 0;
        while let Err(err) = fs::remove_dir_all(destination) {
            attempt_count += 1;
            if err.raw_os_error().unwrap_or_default()
                != ERROR_CODE_VIRTUALIZATION_TEMPORARILY_UNAVAILABLE
                || attempt_count > 5
            {
                warn!("Failed to clean up projection destination: {}", err);
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    Ok(())
}
