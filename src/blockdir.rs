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

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, RwLock, RwLockReadGuard};

use bytes::Bytes;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tracing::{debug, error, warn};
use tracing::{instrument, trace};
use transport::WriteMode;

use crate::compress::snappy::{Compressor, Decompressor};
use crate::counters::Counter;
use crate::monitor::Monitor;
use crate::transport::Transport;
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
///
/// The `BlockDir` object corresponds to the `d` directory in the archive.
///
/// While the `BlockDir` object is open, it knows all the blocks that are present in the archive:
/// they're listed when the `BlockDir` is opened, and the list is updated as blocks are added or removed.
#[derive(Debug)]
pub(crate) struct BlockDir {
    transport: Transport,
    pub stats: BlockDirStats,
    // TODO: There are fancier caches and they might help, but this one works, and Stretto did not work for me.
    cache: RwLock<LruCache<BlockHash, Bytes>>,
    /// All the blocks that are known to be present in the archive.
    exists: RwLock<HashSet<BlockHash>>,
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
    pub(crate) async fn open(transport: Transport) -> Result<BlockDir> {
        // TODO: Take a Monitor here so we can show progress listing blocks.
        /// Cache this many blocks in memory.
        // TODO: Change to a cache that tracks the size of stored blocks?
        // As a safe conservative value, 100 blocks of 20MB each would be 2GB.
        const BLOCK_CACHE_SIZE: usize = 100;

        let exists = list_blocks(&transport).await?;

        Ok(BlockDir {
            transport,
            stats: BlockDirStats::default(),
            cache: RwLock::new(LruCache::new(BLOCK_CACHE_SIZE.try_into().unwrap())),
            exists: RwLock::new(exists),
        })
    }

    pub(crate) async fn create(transport: Transport) -> Result<BlockDir> {
        transport.create_dir("").await?;
        BlockDir::open(transport).await
    }

    pub fn blocks(&'_ self) -> RwLockReadGuard<'_, HashSet<BlockHash>> {
        self.exists.read().unwrap()
    }

    /// Store block data, if it's not already present, and return the hash.
    ///
    /// The block data must be less than the maximum block size.
    pub(crate) async fn store_or_deduplicate(
        &self,
        block_data: Bytes,
        stats: &mut BackupStats,
        monitor: Arc<dyn Monitor>,
    ) -> Result<BlockHash> {
        let hash = BlockHash::hash_bytes(&block_data);
        let uncomp_len = block_data.len() as u64;
        if self.contains(&hash) {
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
        self.transport.create_dir(subdir_relpath(&hex_hash)).await?;
        match self
            .transport
            .write(&relpath, &compressed, WriteMode::CreateNew)
            .await
        {
            Ok(()) => {}
            Err(err) => {
                // We previously checked that the block was not already present. If there is another
                // backup concurrently writing, we might race with it and find here that the block's
                // already been written. However, I'm going to move towards holding a lock for all
                // writes, and then that will never happen.
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
        self.exists.write().unwrap().insert(hash.clone());
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
    pub(crate) fn contains(&self, hash: &BlockHash) -> bool {
        self.exists.read().unwrap().contains(hash)
    }

    /// Returns the compressed on-disk size of a block.
    pub(crate) async fn compressed_size(&self, hash: &BlockHash) -> Result<u64> {
        Ok(self.transport.metadata(&block_relpath(hash)).await?.len)
    }

    /// Read back some content addressed by an [Address] (a block hash, start and end).
    pub(crate) async fn read_address(
        &self,
        address: &Address,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Bytes> {
        let bytes = self.get_block_content(&address.hash, monitor).await?;
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
    pub(crate) async fn get_block_content(
        &self,
        hash: &BlockHash,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Bytes> {
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
        let compressed_bytes = self.transport.read(&block_relpath).await?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = BlockHash::hash_bytes(&decompressed_bytes);
        if actual_hash != *hash {
            return Err(Error::BlockCorrupt { hash: hash.clone() });
        }
        self.cache
            .write()
            .expect("Lock cache")
            .put(hash.clone(), decompressed_bytes.clone());
        self.exists.write().unwrap().insert(hash.clone());
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

    pub(crate) async fn delete_block(&self, hash: &BlockHash) -> Result<()> {
        self.cache.write().expect("Lock cache").pop(hash);
        self.exists.write().unwrap().remove(hash);
        self.transport
            .remove_file(&block_relpath(hash))
            .await
            .map_err(Error::from)
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
        let blocks = self.exists.read().unwrap().clone();
        debug!("Check {} blocks", blocks.len());
        let task = monitor.start_task("Validate blocks".to_string());
        task.set_total(blocks.len());
        let mut taskset = JoinSet::new();
        for hash in blocks.iter() {
            let hash = hash.to_owned();
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
    let compressed_bytes = transport.read(&block_relpath).await?;
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

/// Return an iterator of block subdirectories, in arbitrary order.
///
/// Errors, other than failure to open the directory at all, are logged and discarded.
async fn subdirs(transport: &Transport) -> Result<Vec<String>> {
    let dirs = transport
        .list_dir("")
        .await?
        .into_iter()
        .filter(|entry| entry.kind == Kind::Dir)
        .map(|entry| entry.name)
        .filter(|dirname| {
            let t = dirname.len() == SUBDIR_NAME_CHARS;
            if !t {
                warn!("Unexpected subdirectory in blockdir: {dirname:?}");
            }
            t
        })
        .collect();
    Ok(dirs)
}

/// Return all the blocknames in the blockdir, in arbitrary order.
pub(crate) async fn list_blocks(transport: &Transport) -> Result<HashSet<BlockHash>> {
    let subdirs = subdirs(transport).await?;
    let mut subdir_tasks = JoinSet::new();
    for subdir_name in subdirs {
        let transport = transport.clone();
        subdir_tasks.spawn(async move { transport.list_dir(&subdir_name).await });
    }
    let mut blocks = HashSet::new();
    while let Some(result) = subdir_tasks.join_next().await {
        let result = result.expect("await listdir result");
        match result {
            Ok(entries) => {
                for entry in entries {
                    if entry.is_file() {
                        if entry.len.is_none_or(|a| a == 0) {
                            warn!("Empty block file: {:?}", entry.name);
                            continue;
                        }
                        let Ok(hash) = entry.name.parse() else {
                            warn!("Unexpected block name: {:?}", entry.name);
                            continue;
                        };
                        if !blocks.insert(hash) {
                            warn!("Duplicate block name: {:?}", entry.name);
                        }
                    }
                }
            }
            Err(source) => {
                error!("Error listing blocks: {:?}", source);
                return Err(Error::ListBlocks { source });
            }
        }
    }
    Ok(blocks)
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

    use crate::{monitor::test::TestMonitor, transport::record::Verb};

    use super::*;

    #[tokio::test]
    async fn empty_block_file_counts_as_not_present() {
        // Due to an interruption or system crash we might end up with a block
        // file with 0 bytes. It's not valid compressed data. We just treat
        // the block as not present at all.
        let transport = Transport::temp().enable_record_calls();
        let blockdir = BlockDir::open(transport.clone()).await.unwrap();
        let mut stats = BackupStats::default();
        let monitor = TestMonitor::arc();
        let content = Bytes::from("stuff");
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats, monitor.clone())
            .await
            .unwrap();
        assert_eq!(monitor.get_counter(Counter::BlockWrites), 1);
        assert_eq!(monitor.get_counter(Counter::DeduplicatedBlocks), 0);
        assert!(blockdir.contains(&hash));
        let recording = transport.take_recording();
        dbg!(&recording);
        assert_eq!(
            recording.verb_paths(Verb::Write),
            vec![block_relpath(&hash)],
            "should write the block to the transport"
        );

        // Overwrite the block with an empty file, simulating corruption where the filesystem
        // created the file but lost the content.
        transport
            .write(&block_relpath(&hash), b"", WriteMode::Overwrite)
            .await
            .unwrap();
        let _ = transport.take_recording();

        // Open again to get a fresh cache
        let monitor = TestMonitor::arc();
        let blockdir = BlockDir::open(transport.clone()).await.unwrap();
        assert!(!blockdir.contains(&hash));
        let recording = transport.take_recording();
        dbg!(&recording);
        assert_eq!(
            recording.verb_paths(Verb::ListDir).len(),
            2,
            "should list base and subdirectory"
        );
        assert_eq!(
            recording.verb_paths(Verb::Metadata).len(),
            0,
            "should not get metadata for the block"
        );
        assert_eq!(
            recording.verb_paths(Verb::Write).len(),
            0,
            "should not write the block to the transport because it's now present"
        );

        // If you now store it, it will overwrite the empty file and you'll be able to read it again.
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats, monitor.clone())
            .await
            .unwrap();
        assert_eq!(monitor.get_counter(Counter::BlockWrites), 1);
        assert_eq!(monitor.get_counter(Counter::DeduplicatedBlocks), 0);
        assert!(blockdir.contains(&hash));
        let recording = transport.take_recording();
        assert_eq!(
            recording.verb_paths(Verb::Write).len(),
            1,
            "should write the block to the transport"
        );

        let blockdir = BlockDir::open(transport.clone()).await.unwrap();
        let monitor = TestMonitor::arc();
        let retrieved = blockdir
            .get_block_content(&hash, monitor.clone())
            .await
            .unwrap();
        assert_eq!(content, retrieved);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheHit), 0);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheMiss), 1);
        let recording = transport.take_recording();
        assert_eq!(
            recording.verb_paths(Verb::Read).len(),
            1,
            "should read the block from the transport because it's not cached in memory"
        );
    }

    #[tokio::test]
    async fn store_existing_block_is_not_an_error() {
        let transport = Transport::temp().enable_record_calls();
        let blockdir = BlockDir::open(transport.clone()).await.unwrap();
        let mut stats = BackupStats::default();
        let monitor = TestMonitor::arc();
        let content = Bytes::from("stuff");
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats, monitor.clone())
            .await
            .unwrap();
        assert_eq!(monitor.get_counter(Counter::BlockWrites), 1);
        assert_eq!(monitor.get_counter(Counter::DeduplicatedBlocks), 0);
        assert!(blockdir.contains(&hash));
        let recording = transport.take_recording();
        assert_eq!(
            recording.verb_paths(Verb::Write).len(),
            1,
            "should write the block to the transport"
        );
        assert_eq!(
            recording.verb_paths(Verb::CreateDir).len(),
            1,
            "should create a subdirectory for the block"
        );

        // Open again to get a fresh cache
        let blockdir = BlockDir::open(transport.clone()).await.unwrap();
        let monitor = TestMonitor::arc();
        let recording = transport.take_recording();
        assert_eq!(
            recording.verb_paths(Verb::ListDir).len(),
            2,
            "Loading a blockdir should list the base and subdirectory"
        );

        let _hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats, monitor.clone())
            .await
            .unwrap();
        assert_eq!(monitor.get_counter(Counter::BlockWrites), 0);
        assert_eq!(monitor.get_counter(Counter::DeduplicatedBlocks), 1);
        let recording = transport.take_recording();
        assert_eq!(
            recording.calls,
            [],
            "Storing existing block should do no IO",
        );
    }

    #[tokio::test]
    async fn blocks_async() {
        let tempdir = TempDir::new().unwrap();
        let blockdir = BlockDir::open(Transport::local(tempdir.path()))
            .await
            .unwrap();
        let mut stats = BackupStats::default();
        let monitor = TestMonitor::arc();

        assert_eq!(blockdir.blocks().len(), 0);

        let hash = blockdir
            .store_or_deduplicate(Bytes::from("stuff"), &mut stats, monitor.clone())
            .await
            .unwrap();

        let blocks = blockdir.blocks().iter().cloned().collect::<Vec<_>>();
        assert_eq!(blocks, [hash]);
    }

    #[tokio::test]
    async fn temp_files_are_not_returned_as_blocks() {
        let tempdir = TempDir::new().unwrap();
        let subdir = tempdir.path().join(subdir_relpath("123"));
        create_dir(&subdir).unwrap();
        // Write a temp file as was created by earlier versions of the code.
        write(subdir.join("tmp123123123"), b"123").unwrap();

        let blockdir = BlockDir::open(Transport::local(tempdir.path()))
            .await
            .unwrap();
        let blocks = blockdir.blocks();
        assert_eq!(
            blocks.len(),
            0,
            "Temp file should not be returned as a block"
        );
    }

    #[tokio::test]
    async fn cache_hit() {
        let transport = Transport::temp().enable_record_calls();
        let blockdir = BlockDir::open(transport.clone()).await.unwrap();
        let mut stats = BackupStats::default();
        let content = Bytes::from("stuff");
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats, TestMonitor::arc())
            .await
            .unwrap();
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 0);

        assert!(blockdir.contains(&hash));

        let _recording = transport.take_recording();
        let monitor = TestMonitor::arc();
        let retrieved = blockdir
            .get_block_content(&hash, monitor.clone())
            .await
            .unwrap();
        assert_eq!(content, retrieved);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheHit), 1);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheMiss), 0);
        let recording = transport.take_recording();
        assert_eq!(
            recording.verb_paths(Verb::Read).len(),
            0,
            "should not read the block from the transport because it's cached in memory"
        );
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 1); // hit against the value written

        let retrieved = blockdir
            .get_block_content(&hash, monitor.clone())
            .await
            .unwrap();
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheHit), 2);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheMiss), 0);
        assert_eq!(content, retrieved);
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 2); // hit again
        let recording = transport.take_recording();
        assert_eq!(
            recording.verb_paths(Verb::Read).len(),
            0,
            "should not read the block from the transport because it's cached in memory"
        );
    }

    #[tokio::test]
    async fn existence_cache_hit() {
        let transport = Transport::temp().enable_record_calls();
        let blockdir = BlockDir::open(transport.clone()).await.unwrap();
        let mut stats = BackupStats::default();
        let content = Bytes::from("stuff");
        let monitor = TestMonitor::arc();
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats, monitor.clone())
            .await
            .unwrap();

        // reopen
        let _recording = transport.take_recording();
        let monitor = TestMonitor::arc();
        let blockdir = BlockDir::open(transport.clone()).await.unwrap();
        assert!(blockdir.contains(&hash));

        assert!(blockdir.contains(&hash));

        assert!(blockdir.contains(&hash));
        let recording = transport.take_recording();
        assert_eq!(
            recording.verb_paths(Verb::ListDir).len(),
            2,
            "should list base and subdirectory"
        );
        assert_eq!(
            recording.verb_paths(Verb::Metadata).len(),
            0,
            "should not get metadata for the block"
        );

        // actually reading the content is a miss and requires reading the block from the transport
        let retrieved = blockdir
            .get_block_content(&hash, monitor.clone())
            .await
            .unwrap();
        assert_eq!(content, retrieved);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheMiss), 1);
        assert_eq!(monitor.get_counter(Counter::BlockContentCacheHit), 0);
        assert_eq!(
            blockdir.stats.cache_hit.load(Relaxed),
            0,
            "first read should miss the cache"
        );
        let recording = transport.take_recording();
        assert_eq!(
            recording.verb_paths(Verb::Read).len(),
            1,
            "should read the block from the transport because it's not cached in memory"
        );
        assert_eq!(
            recording.verb_paths(Verb::ListDir).len(),
            0,
            "should list base and subdirectory"
        );
        assert_eq!(
            recording.verb_paths(Verb::Metadata).len(),
            0,
            "should not get metadata for the block"
        );
    }
}
