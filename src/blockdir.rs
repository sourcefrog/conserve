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
use std::convert::TryInto;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use bytes::Bytes;
use lru::LruCache;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};
use tracing::{instrument, trace};

use crate::backup::BackupStats;
use crate::blockhash::BlockHash;
use crate::compress::snappy::{Compressor, Decompressor};
use crate::progress::{Bar, Progress};
use crate::transport::{ListDir, Transport};
use crate::*;

const BLOCKDIR_FILE_NAME_LEN: usize = crate::BLAKE_HASH_SIZE_BYTES * 2;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

/// Cache this many blocks in memory.
const BLOCK_CACHE_SIZE: usize = (1 << 30) / MAX_BLOCK_SIZE;

const EXISTENCE_CACHE_SIZE: usize = (64 << 20) / BLOCKDIR_FILE_NAME_LEN;

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
pub struct BlockDir {
    transport: Arc<dyn Transport>,
    pub stats: BlockDirStats,
    // TODO: There are fancier caches and they might help, but this one works, and Stretto did not work for me.
    cache: RwLock<LruCache<BlockHash, Bytes>>,
    /// True if we know that this block exists, even if we don't have its content.
    ///
    /// This does _not_ contain keys that are in `cache`.
    exists: RwLock<LruCache<BlockHash, ()>>,
}

/// Returns the transport-relative subdirectory name.
fn subdir_relpath(block_hash: &str) -> &str {
    &block_hash[..SUBDIR_NAME_CHARS]
}

/// Return the transport-relative file for a given hash.
pub fn block_relpath(hash: &BlockHash) -> String {
    let hash_hex = hash.to_string();
    format!("{}/{}", subdir_relpath(&hash_hex), hash_hex)
}

impl BlockDir {
    pub fn open(transport: Arc<dyn Transport>) -> BlockDir {
        BlockDir {
            transport,
            stats: BlockDirStats::default(),
            cache: RwLock::new(LruCache::new(BLOCK_CACHE_SIZE.try_into().unwrap())),
            exists: RwLock::new(LruCache::new(EXISTENCE_CACHE_SIZE.try_into().unwrap())),
        }
    }

    pub fn create(transport: Arc<dyn Transport>) -> Result<BlockDir> {
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
    ) -> Result<BlockHash> {
        let hash = BlockHash::hash_bytes(&block_data);
        let uncomp_len = block_data.len() as u64;
        if self.contains(&hash)? {
            stats.deduplicated_blocks += 1;
            stats.deduplicated_bytes += uncomp_len;
            return Ok(hash);
        }
        let compressed = Compressor::new().compress(&block_data)?;
        self.cache
            .write()
            .expect("Lock cache")
            .put(hash.clone(), block_data);
        let comp_len: u64 = compressed.len().try_into().unwrap();
        let hex_hash = hash.to_string();
        let relpath = block_relpath(&hash);
        self.transport.create_dir(subdir_relpath(&hex_hash))?;
        self.transport.write_file(&relpath, &compressed)?;
        stats.written_blocks += 1;
        stats.uncompressed_bytes += uncomp_len;
        stats.compressed_bytes += comp_len;
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
    pub fn contains(&self, hash: &BlockHash) -> Result<bool> {
        if self.cache.read().expect("Lock cache").contains(hash)
            || self.exists.read().unwrap().contains(hash)
        {
            self.stats.cache_hit.fetch_add(1, Relaxed);
            return Ok(true);
        }
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
    pub fn compressed_size(&self, hash: &BlockHash) -> Result<u64> {
        Ok(self.transport.metadata(&block_relpath(hash))?.len)
    }

    /// Read back some content addressed by an [Address] (a block hash, start and end).
    pub fn read_address(&self, address: &Address) -> Result<Bytes> {
        let bytes = self.get_block_content(&address.hash)?;
        let len = address.len as usize;
        let start = address.start as usize;
        let end = start + len;
        let actual_len = bytes.len();
        if end > actual_len {
            return Err(Error::AddressTooLong {
                address: address.to_owned(),
                actual_len,
            });
        }
        Ok(bytes.slice(start..end))
    }

    /// Return the entire contents of the block.
    ///
    /// Checks that the hash is correct with the contents.
    #[instrument(skip(self))]
    pub fn get_block_content(&self, hash: &BlockHash) -> Result<Bytes> {
        if let Some(hit) = self.cache.write().expect("Lock cache").get(hash) {
            self.stats.cache_hit.fetch_add(1, Relaxed);
            trace!("Block cache hit");
            return Ok(hit.clone());
        }
        let mut decompressor = Decompressor::new();
        let block_relpath = block_relpath(hash);
        let compressed_bytes = self.transport.read_file(&block_relpath)?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = BlockHash::hash_bytes(&decompressed_bytes);
        if actual_hash != *hash {
            error!(%hash, %actual_hash, %block_relpath, "Block file has wrong hash");
            return Err(Error::BlockCorrupt { hash: hash.clone() });
        }
        self.cache
            .write()
            .expect("Lock cache")
            .put(hash.clone(), decompressed_bytes.clone());
        self.exists.write().unwrap().pop(hash);
        self.stats.read_blocks.fetch_add(1, Relaxed);
        self.stats
            .read_block_compressed_bytes
            .fetch_add(compressed_bytes.len(), Relaxed);
        self.stats
            .read_block_uncompressed_bytes
            .fetch_add(decompressed_bytes.len(), Relaxed);
        Ok(decompressed_bytes)
    }

    pub fn delete_block(&self, hash: &BlockHash) -> Result<()> {
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

    /// Return all the blocknames in the blockdir, in arbitrary order.
    pub fn iter_block_names(&self) -> Result<impl Iterator<Item = BlockHash>> {
        // TODO: Read subdirs in parallel.
        let transport = self.transport.clone();
        Ok(self
            .subdirs()?
            .into_iter()
            .map(move |subdir_name| transport.list_dir(&subdir_name))
            .filter_map(|iter_or| {
                if let Err(ref err) = iter_or {
                    error!(%err, "Error listing block subdirectory");
                }
                iter_or.ok()
            })
            .flat_map(|ListDir { files, .. }| files)
            .filter(|name| name.len() == BLOCKDIR_FILE_NAME_LEN && !name.starts_with(TMP_PREFIX))
            .filter_map(|name| name.parse().ok()))
    }

    /// Return all the blocknames in the blockdir, while showing progress.
    pub fn block_names_set(&self) -> Result<HashSet<BlockHash>> {
        // TODO: We could estimate time remaining by accounting for how
        // many prefixes are present and how many have been read.
        let bar = Bar::new();
        Ok(self
            .iter_block_names()?
            .enumerate()
            .map(|(count, hash)| {
                bar.post(Progress::ListBlocks { count });
                hash
            })
            .collect())
    }

    /// Check format invariants of the BlockDir.
    ///
    /// Return a dict describing which blocks are present, and the length of their uncompressed
    /// data.
    pub fn validate(&self) -> Result<HashMap<BlockHash, usize>> {
        // TODO: In the top-level directory, no files or directories other than prefix
        // directories of the right length.
        // TODO: Test having a block with the right compression but the wrong contents.
        // TODO: Warn on blocks in the wrong subdir.
        debug!("Start list blocks");
        let blocks = self.block_names_set()?;
        let total_blocks = blocks.len();
        debug!("Check {total_blocks} blocks");
        let blocks_done = AtomicUsize::new(0);
        let bytes_done = AtomicU64::new(0);
        let start = Instant::now();
        let task = Bar::new();
        let block_lens = blocks
            .into_par_iter()
            .flat_map(|hash| match self.get_block_content(&hash) {
                Ok(bytes) => {
                    let len = bytes.len();
                    let len64 = len as u64;
                    task.post(Progress::ValidateBlocks {
                        blocks_done: blocks_done.fetch_add(1, Ordering::Relaxed) + 1,
                        total_blocks,
                        bytes_done: bytes_done.fetch_add(len64, Ordering::Relaxed) + len64,
                        start,
                    });
                    Some((hash, len))
                }
                Err(err) => {
                    error!(%err, %hash, "Error reading block content");
                    None
                }
            })
            .collect();
        Ok(block_lens)
    }
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
    use std::fs::OpenOptions;

    use crate::transport::open_local_transport;

    use super::*;
    use tempfile::TempDir;
    #[test]
    fn empty_block_file_counts_as_not_present() {
        let tempdir = TempDir::new().unwrap();
        let blockdir = BlockDir::open(open_local_transport(tempdir.path()).unwrap());
        let mut stats = BackupStats::default();
        let hash = blockdir
            .store_or_deduplicate(Bytes::from("stuff"), &mut stats)
            .unwrap();
        assert!(blockdir.contains(&hash).unwrap());

        // Open again to get a fresh cache
        let blockdir = BlockDir::open(open_local_transport(tempdir.path()).unwrap());
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(false)
            .open(tempdir.path().join(block_relpath(&hash)))
            .expect("Truncate block");
        assert!(!blockdir.contains(&hash).unwrap());
    }

    #[test]
    fn cache_hit() {
        let tempdir = TempDir::new().unwrap();
        let blockdir = BlockDir::open(open_local_transport(tempdir.path()).unwrap());
        let mut stats = BackupStats::default();
        let content = Bytes::from("stuff");
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats)
            .unwrap();
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 0);

        assert!(blockdir.contains(&hash).unwrap());
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 1);

        let retrieved = blockdir.get_block_content(&hash).unwrap();
        assert_eq!(content, retrieved);
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 2); // hit against the value written

        let retrieved = blockdir.get_block_content(&hash).unwrap();
        assert_eq!(content, retrieved);
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 3); // hit again
    }

    #[test]
    fn existence_cache_hit() {
        let tempdir = TempDir::new().unwrap();
        let blockdir = BlockDir::open(open_local_transport(tempdir.path()).unwrap());
        let mut stats = BackupStats::default();
        let content = Bytes::from("stuff");
        let hash = blockdir
            .store_or_deduplicate(content.clone(), &mut stats)
            .unwrap();

        // reopen
        let blockdir = BlockDir::open(open_local_transport(tempdir.path()).unwrap());
        assert!(blockdir.contains(&hash).unwrap());
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 0);
        assert!(blockdir.contains(&hash).unwrap());
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 1);
        assert!(blockdir.contains(&hash).unwrap());
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 2);

        // actually reading the content is a miss
        let retrieved = blockdir.get_block_content(&hash).unwrap();
        assert_eq!(content, retrieved);
        assert_eq!(blockdir.stats.cache_hit.load(Relaxed), 2); // hit again
    }
}
