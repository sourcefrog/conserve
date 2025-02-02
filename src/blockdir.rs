// Conserve backup system.
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

//! File contents are stored in data blocks.
//!
//! Data blocks are stored compressed, and identified by the hash of their uncompressed
//! contents.
//!
//! The contents of a file is identified by an Address, which says which block holds the data,
//! and which range of uncompressed bytes.
//!
//! The structure is: archive > blockdir > subdir > file.

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tracing::{debug, warn};
use tracing::{instrument, trace};
use transport::WriteMode;

use crate::compress::snappy::{Compressor, Decompressor};
use crate::counters::Counter;
use crate::monitor::Monitor;
use crate::transport::{ListDir, Transport};
use crate::*;

// const BLOCKDIR_FILE_NAME_LEN: usize = crate::BLAKE_HASH_SIZE_BYTES * 2;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

/// Points to some compressed data inside the block dir.
///
/// Identifiers are: which file contains it, at what (pre-compression) offset,
/// and what (pre-compression) length.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    /// Hash of the block storing this info.
    pub hash: BlockHash,

    /// Position in this block where data begins.
    #[serde(default)]
    #[serde(skip_serializing_if = "crate::misc::zero_u64")]
    pub start: u64,

    /// Length of this block to be used.
    pub len: u64,
}

/// A readable, writable directory within a band holding data blocks.
#[derive(Debug)]
pub(crate) struct BlockDir {
    transport: Transport,
    pub stats: BlockDirStats,
    // TODO: There are fancier caches and they might help, but this one works, and Stretto did not work for me.
    cache: RwLock<LruCache<BlockHash, Bytes>>,
    /// Presence means that we know that this block exists, even if we don't have its content.
    exists: RwLock<LruCache<BlockHash, ()>>,
}

/// Returns the transport-relative subdirectory name.
fn subdir_relpath(block_hash: &str) -> &str {
    &block_hash[..SUBDIR_NAME_CHARS]
}

/// Return the transport-relative file for a given hash.
// This is exposed for testing, so that damage tests can determine
// which files to damage.
pub fn block_relpath(hash: &BlockHash) -> String {
    let hash_hex = hash.to_string();
    format!("{}/{}", subdir_relpath(&hash_hex), hash_hex)
}

impl BlockDir {
    pub(crate) fn open(transport: Transport) -> BlockDir {
        /// Cache this many blocks in memory.
        // TODO: Change to a cache that tracks the size of stored blocks?
        // As a safe conservative value, 100 blocks of 20MB each would be 2GB.
        const BLOCK_CACHE_SIZE: usize = 100;

        /// Remember the existence of this many blocks, even if we don't have their content.
        const EXISTENCE_CACHE_SIZE: usize = (64 << 20) / BLAKE_HASH_SIZE_BYTES;

        BlockDir {
            transport,
            stats: BlockDirStats::default(),
            cache: RwLock::new(LruCache::new(BLOCK_CACHE_SIZE.try_into().unwrap())),
            exists: RwLock::new(LruCache::new(EXISTENCE_CACHE_SIZE.try_into().unwrap())),
        }
    }

    pub(crate) fn create(transport: Transport) -> Result<BlockDir> {
        transport.create_dir("")?;
        Ok(BlockDir::open(transport))
    }

    /// Store block data, if it's not already present, and return the hash.
    ///
    /// The block data must be less than the maximum block size.
    pub(crate) fn store_or_deduplicate(
        &self,
        block_data: Bytes,
        stats: &mut BackupStats,
        monitor: Arc<dyn Monitor>,
    ) -> Result<BlockHash> {
        let hash = BlockHash::hash_bytes(&block_data);
        let uncomp_len = block_data.len() as u64;
        if self.contains(&hash, monitor.clone())? {
            stats.deduplicated_blocks += 1;
            stats.deduplicated_bytes += uncomp_len;
            monitor.count(Counter::DeduplicatedBlocks, 1);
            monitor.count(Counter::DeduplicatedBlockBytes, block_data.len());
            return Ok(hash);
        }
        let compressed = Compressor::new().compress(&block_data)?;
        monitor.count(Counter::BlockWriteUncompressedBytes, block_data.len());
        let comp_len: u64 = compressed.len().try_into().unwrap();
        let hex_hash = hash.to_string();
        let relpath = block_relpath(&hash);
        self.transport.create_dir(subdir_relpath(&hex_hash))?;
        match self
            .transport
            .write(&relpath, &compressed, WriteMode::CreateNew)
        {
            Ok(()) => {}
            Err(err) if err.kind() == transport::ErrorKind::AlreadyExists => {
                // let's assume the contents are correct
            }
            Err(err) => {
                warn!(?err, ?hash, "Error writing block");
                return Err(err.into());
            }
        }
        stats.written_blocks += 1;
        stats.uncompressed_bytes += uncomp_len;
        stats.compressed_bytes += comp_len;
        monitor.count(Counter::BlockWrites, 1);
        monitor.count(Counter::BlockWriteCompressedBytes, compressed.len());
        // Only update caches after everything succeeded
        self.cache
            .write()
            .expect("Lock cache")
            .put(hash.clone(), block_data);
        self.exists.write().unwrap().push(hash.clone(), ());
        Ok(hash)
    }

    /// True if the named block is present and apparently in this blockdir.
    ///
    /// Empty block files should never normally occur, because the index doesn't
    /// point to empty blocks and anyhow the compression method would expand an
    /// empty block to a non-empty compressed form. However, it's possible for
    /// an interrupted operation on a local filesystem to leave an empty file.
    /// So, these are specifically treated as missing, so there's a chance to heal
    /// them later.
    pub(crate) fn contains(&self, hash: &BlockHash, monitor: Arc<dyn Monitor>) -> Result<bool> {
        if self.cache.read().expect("Lock cache").contains(hash)
            || self.exists.read().unwrap().contains(hash)
        {
            monitor.count(Counter::BlockExistenceCacheHit, 1);
            self.stats.cache_hit.fetch_add(1, Relaxed);
            return Ok(true);
        }
        monitor.count(Counter::BlockExistenceCacheMiss, 1);
        match self.transport.metadata(&block_relpath(hash)) {
            Err(err) if err.is_not_found() => Ok(false),
            Err(err) => {
                warn!(?err, ?hash, "Error checking presence of block");
                Err(err.into())
            }
            Ok(metadata) if metadata.kind == Kind::File && metadata.len > 0 => {
                self.exists.write().unwrap().put(hash.clone(), ());
                Ok(true)
            }
            Ok(_) => Ok(false),
        }
    }

    /// Returns the compressed on-disk size of a block.
    pub(crate) fn compressed_size(&self, hash: &BlockHash) -> Result<u64> {
        Ok(self.transport.metadata(&block_relpath(hash))?.len)
    }

    /// Read back some content addressed by an [Address] (a block hash, start and end).
    pub(crate) fn read_address(
        &self,
        address: &Address,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Bytes> {
        let bytes = self.get_block_content(&address.hash, monitor)?;
        let len = address.len as usize;
        let start = address.start as usize;
        let end = start + len;
        let actual_len = bytes.len();
        if end > actual_len {
            return Err(Error::BlockTooShort {
                hash: address.hash.clone(),
                actual_len,
                referenced_len: len,
            });
        }
        Ok(bytes.slice(start..end))
    }

    /// Return the entire contents of the block.
    ///
    /// Checks that the hash is correct with the contents.
    #[instrument(skip(self, monitor))]
    pub(crate) fn get_block_content(
        &self,
        hash: &BlockHash,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Bytes> {
        if let Some(hit) = self.cache.write().expect("Lock cache").get(hash) {
            monitor.count(Counter::BlockContentCacheHit, 1);
            self.stats.cache_hit.fetch_add(1, Relaxed);
            trace!("Block cache hit");
            return Ok(hit.clone());
        }
        monitor.count(Counter::BlockContentCacheMiss, 1);
        let mut decompressor = Decompressor::new();
        let block_relpath = block_relpath(hash);
        let compressed_bytes = self.transport.read(&block_relpath)?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = BlockHash::hash_bytes(&decompressed_bytes);
        if actual_hash != *hash {
            return Err(Error::BlockCorrupt { hash: hash.clone() });
        }
        self.cache
            .write()
            .expect("Lock cache")
            .put(hash.clone(), decompressed_bytes.clone());
        self.exists.write().unwrap().put(hash.clone(), ());
        self.stats.read_blocks.fetch_add(1, Relaxed);
        monitor.count(Counter::BlockReads, 1);
        self.stats
            .read_block_compressed_bytes
            .fetch_add(compressed_bytes.len(), Relaxed);
        monitor.count(Counter::BlockReadCompressedBytes, compressed_bytes.len());
        self.stats
            .read_block_uncompressed_bytes
            .fetch_add(decompressed_bytes.len(), Relaxed);
        monitor.count(
            Counter::BlockReadUncompressedBytes,
            decompressed_bytes.len(),
        );
        Ok(decompressed_bytes)
    }

    /// Return the entire contents of the block.
    ///
    /// Checks that the hash is correct with the contents.
    #[instrument(skip(self, monitor))]
    pub async fn get_async(&self, hash: &BlockHash, monitor: Arc<dyn Monitor>) -> Result<Bytes> {
        // TODO: Tokio locks on caches
        if let Some(hit) = self.cache.write().expect("Lock cache").get(hash) {
            monitor.count(Counter::BlockContentCacheHit, 1);
            self.stats.cache_hit.fetch_add(1, Relaxed);
            trace!("Block cache hit");
            return Ok(hit.clone());
        }
        monitor.count(Counter::BlockContentCacheMiss, 1);
        let mut decompressor = Decompressor::new();
        let block_relpath = block_relpath(hash);
        let compressed_bytes = self.transport.read_async(&block_relpath).await?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = BlockHash::hash_bytes(&decompressed_bytes);
        if actual_hash != *hash {
            return Err(Error::BlockCorrupt { hash: hash.clone() });
        }
        self.cache
            .write()
            .expect("Lock cache")
            .put(hash.clone(), decompressed_bytes.clone());
        self.exists
            .write()
            .expect("Lock existence cache")
            .put(hash.clone(), ());
        self.stats.read_blocks.fetch_add(1, Relaxed);
        monitor.count(Counter::BlockReads, 1);
        self.stats
            .read_block_compressed_bytes
            .fetch_add(compressed_bytes.len(), Relaxed);
        monitor.count(Counter::BlockReadCompressedBytes, compressed_bytes.len());
        self.stats
            .read_block_uncompressed_bytes
            .fetch_add(decompressed_bytes.len(), Relaxed);
        monitor.count(
            Counter::BlockReadUncompressedBytes,
            decompressed_bytes.len(),
        );
        Ok(decompressed_bytes)
    }

    pub(crate) fn delete_block(&self, hash: &BlockHash) -> Result<()> {
        self.cache.write().expect("Lock cache").pop(hash);
        self.exists.write().unwrap().pop(hash);
        self.transport
            .remove_file(&block_relpath(hash))
            .map_err(Error::from)
    }

    /// Return an iterator of block subdirectories, in arbitrary order.
    ///
    /// Errors, other than failure to open the directory at all, are logged and discarded.
    fn subdirs(&self) -> Result<Vec<String>> {
        let ListDir { mut dirs, .. } = self.transport.list_dir("")?;
        dirs.retain(|dirname| {
            if dirname.len() == SUBDIR_NAME_CHARS {
                true
            } else {
                warn!("Unexpected subdirectory in blockdir: {dirname:?}");
                false
            }
        });
        Ok(dirs)
    }

    /// Return an iterator of block subdirectories, in arbitrary order.
    ///
    /// Errors, other than failure to open the directory at all, are logged and discarded.
    async fn subdirs_async(&self) -> Result<Vec<String>> {
        let ListDir { mut dirs, .. } = self.transport.list_dir_async("").await?;
        dirs.retain(|dirname| {
            if dirname.len() == SUBDIR_NAME_CHARS {
                true
            } else {
                warn!("Unexpected subdirectory in blockdir: {dirname:?}");
                false
            }
        });
        Ok(dirs)
    }

    /// Return all the blocknames in the blockdir, in arbitrary order.
    pub(crate) fn blocks(&self, monitor: Arc<dyn Monitor>) -> Result<Vec<BlockHash>> {
        let transport = self.transport.clone();
        let task = monitor.start_task("List block subdir".to_string());
        let subdirs = self.subdirs()?;
        task.set_total(subdirs.len());
        Ok(subdirs
            .into_iter()
            .map(move |subdir_name| {
                let r = transport.list_dir(&subdir_name);
                task.increment(1);
                r
            })
            .filter_map(move |iter_or| match iter_or {
                Err(source) => {
                    monitor.error(Error::ListBlocks { source });
                    None
                }
                Ok(ListDir { files, .. }) => Some(files),
            })
            .flatten()
            .filter_map(|name| // drop any invalid names, including temp files
                // TODO: Report errors on bad names?
                name.parse().ok())
            .collect())
    }

    /// Return all the blocknames in the blockdir, in arbitrary order.
    pub(crate) async fn blocks_async(&self, monitor: Arc<dyn Monitor>) -> Result<Vec<BlockHash>> {
        let transport = self.transport.clone();
        let task = monitor.start_task("List block subdir".to_string());
        let subdirs = self.subdirs_async().await?;
        task.set_total(subdirs.len());
        let mut subdir_tasks = JoinSet::new();
        for subdir_name in subdirs {
            let transport = transport.clone();
            let my_task = task.clone();
            subdir_tasks.spawn(async move {
                let r = transport.list_dir_async(&subdir_name).await;
                my_task.increment(1);
                r
            });
        }
        let mut blocks = vec![];
        while let Some(result) = subdir_tasks.join_next().await {
            let result = result.expect("await listdir result");
            match result {
                Ok(ListDir { files, .. }) => {
                    blocks.extend(files.into_iter().filter_map(|name|
                        // just drop invalid names, for now
                        name.parse().ok()));
                }
                Err(source) => {
                    monitor.error(Error::ListBlocks { source });
                }
            }
        }
        Ok(blocks)
    }

    /// Check format invariants of the BlockDir.
    ///
    /// Return a dict describing which blocks are present, and the length of their uncompressed
    /// data.
    pub(crate) async fn validate(
        &self,
        monitor: Arc<dyn Monitor>,
    ) -> Result<HashMap<BlockHash, usize>> {
        // TODO: In the top-level directory, no files or directories other than prefix
        // directories of the right length.
        // TODO: Test having a block with the right compression but the wrong contents.
        // TODO: Warn on blocks in the wrong subdir.
        debug!("Start list blocks");
        let blocks = self.blocks_async(monitor.clone()).await?;
        debug!("Check {} blocks", blocks.len());
        let task = monitor.start_task("Validate blocks".to_string());
        task.set_total(blocks.len());
        let mut taskset = JoinSet::new();
        for hash in blocks {
            let monitor = monitor.clone();
            let task = task.clone();
            let transport = self.transport.clone();
            taskset.spawn(async move {
                // get_async_uncached checks that the hash is correct
                task.increment(1);
                match get_async_uncached(&transport, hash.clone(), monitor.clone()).await {
                    Ok(bytes) => Some((hash, bytes.len())),
                    Err(err) => {
                        monitor.error(err);
                        None
                    }
                }
            });
        }
        let block_lens = taskset
            .join_all()
            .await
            .into_iter()
            .flatten()
            .collect::<HashMap<_, _>>();
        Ok(block_lens)
    }
}

// This exists as a non-associated function to allow it to be used in the async
// version of validate, without problems of holding a reference to the BlockDir.
async fn get_async_uncached(
    transport: &Transport,
    hash: BlockHash,
    monitor: Arc<dyn Monitor>,
) -> Result<Bytes> {
    let block_relpath = block_relpath(&hash);
    let compressed_bytes = transport.read_async(&block_relpath).await?;
    let decompressed_bytes = Decompressor::new().decompress(&compressed_bytes)?;
    let actual_hash = BlockHash::hash_bytes(&decompressed_bytes);
    if actual_hash != hash {
        return Err(Error::BlockCorrupt { hash });
    }
    monitor.count(Counter::BlockReads, 1);
    monitor.count(Counter::BlockReadCompressedBytes, compressed_bytes.len());
    monitor.count(
        Counter::BlockReadUncompressedBytes,
        decompressed_bytes.len(),
    );
    trace!(?hash, len = decompressed_bytes.len(), "Read block complete");
    Ok(decompressed_bytes)
}

#[derive(Debug, Default)]
pub struct BlockDirStats {
    pub read_blocks: AtomicUsize,
    pub read_block_compressed_bytes: AtomicUsize,
    pub read_block_uncompressed_bytes: AtomicUsize,
    pub cache_hit: AtomicUsize,
}

#[cfg(test)]
mod test {
    use std::fs::{create_dir, write};

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use crate::monitor::test::TestMonitor;

    use super::*;

    #[test]
    fn empty_block_file_counts_as_not_present() {
        // Due to an interruption or system crash we might end up with a block
        // file with 0 bytes. It's not valid compressed data. We just treat
        // the block as not present at all.
        let transport = Transport::temp();
        let blockdir = BlockDir::open(transport.clone());
        let mut stats = BackupStats::default();
        let monitor = TestMonitor::arc();
        let hash = blockdir
            .store_or_deduplicate(Bytes::from("stuff"), &mut stats, monitor.clone())
            .unwrap();
        assert_eq!(monitor.get_counter(Counter::BlockWrites), 1);
        assert_eq!(monitor.get_counter(Counter::DeduplicatedBlocks), 0);
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheMiss), 1);
        assert!(blockdir.contains(&hash, monitor.clone()).unwrap());
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheMiss), 1);
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheHit), 1); // Since we just wrote it, we know it's there.

        // Open again to get a fresh cache
        let blockdir = BlockDir::open(transport.clone());
        let monitor = TestMonitor::arc();
        transport
            .write(&block_relpath(&hash), b"", WriteMode::Overwrite)
            .unwrap();
        assert!(!blockdir.contains(&hash, monitor.clone()).unwrap());
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheHit), 0);
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheMiss), 1);
    }

    #[tokio::test]
    async fn blocks_async() {
        let tempdir = TempDir::new().unwrap();
        let blockdir = BlockDir::open(Transport::local(tempdir.path()));
        let mut stats = BackupStats::default();
        let monitor = TestMonitor::arc();

        let initial_blocks = blockdir.blocks_async(monitor.clone()).await.unwrap();
        assert_eq!(initial_blocks, []);

        let hash = blockdir
            .store_or_deduplicate(Bytes::from("stuff"), &mut stats, monitor.clone())
            .unwrap();

        let blocks = blockdir.blocks_async(monitor.clone()).await.unwrap();
        assert_eq!(blocks, [hash]);
    }

    #[test]
    fn temp_files_are_not_returned_as_blocks() {
        let tempdir = TempDir::new().unwrap();
        let blockdir = BlockDir::open(Transport::local(tempdir.path()));
        let monitor = TestMonitor::arc();
        let subdir = tempdir.path().join(subdir_relpath("123"));
        create_dir(&subdir).unwrap();
        // Write a temp file as was created by earlier versions of the code.
        write(subdir.join("tmp123123123"), b"123").unwrap();
        let blocks = blockdir.blocks(monitor.clone()).unwrap();
        assert_eq!(blocks, []);
    }

    #[test]
    fn cache_hit() {
        let blockdir = BlockDir::open(Transport::temp());
        let mut stats = BackupStats::default();
        let content = Bytes::from("stuff");
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats, TestMonitor::arc())
            .unwrap();
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 0);

        let monitor = TestMonitor::arc();
        assert!(blockdir.contains(&hash, monitor.clone()).unwrap());
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 1);
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheHit), 1);

        let monitor = TestMonitor::arc();
        let retrieved = blockdir.get_block_content(&hash, monitor.clone()).unwrap();
        assert_eq!(content, retrieved);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheHit), 1);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheMiss), 0);
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 2); // hit against the value written

        let retrieved = blockdir.get_block_content(&hash, monitor.clone()).unwrap();
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheHit), 2);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheMiss), 0);
        assert_eq!(content, retrieved);
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 3); // hit again
    }

    #[test]
    fn existence_cache_hit() {
        let transport = Transport::temp();
        let blockdir = BlockDir::open(transport.clone());
        let mut stats = BackupStats::default();
        let content = Bytes::from("stuff");
        let monitor = TestMonitor::arc();
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats, monitor.clone())
            .unwrap();

        // reopen
        let monitor = TestMonitor::arc();
        let blockdir = BlockDir::open(transport.clone());
        assert!(blockdir.contains(&hash, monitor.clone()).unwrap());
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 0);
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheHit), 0);

        assert!(blockdir.contains(&hash, monitor.clone()).unwrap());
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 1);
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheHit), 1);

        assert!(blockdir.contains(&hash, monitor.clone()).unwrap());
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 2);
        assert_eq!(monitor.get_counter(Counter::BlockExistenceCacheHit), 2);

        // actually reading the content is a miss
        let retrieved = blockdir.get_block_content(&hash, monitor.clone()).unwrap();
        assert_eq!(content, retrieved);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheMiss), 1);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheHit), 0);
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 2); // hit again
    }
}
