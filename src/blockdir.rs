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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ::metrics::{counter, histogram, increment_counter};
use bytes::Bytes;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

use crate::backup::BackupStats;
use crate::blockhash::BlockHash;
use crate::compress::snappy::{Compressor, Decompressor};
use crate::progress::{Bar, Progress};
use crate::transport::{ListDir, Transport};
use crate::*;

const BLOCKDIR_FILE_NAME_LEN: usize = crate::BLAKE_HASH_SIZE_BYTES * 2;

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
#[derive(Clone, Debug)]
pub struct BlockDir {
    transport: Arc<dyn Transport>,
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
        BlockDir { transport }
    }

    pub fn create(transport: Arc<dyn Transport>) -> Result<BlockDir> {
        transport.create_dir("")?;
        Ok(BlockDir { transport })
    }

    /// Store block data, if it's not already present, and return the hash.
    ///
    /// The block data must be less than the maximum block size.
    pub(crate) fn store_or_deduplicate(
        &mut self,
        block_data: &[u8],
        stats: &mut BackupStats,
    ) -> Result<BlockHash> {
        let hash = BlockHash::hash_bytes(block_data);
        let uncomp_len = block_data.len() as u64;
        if self.contains(&hash)? {
            increment_counter!("conserve.block.matches");
            stats.deduplicated_blocks += 1;
            counter!("conserve.block.matched_bytes", uncomp_len);
            stats.deduplicated_bytes += uncomp_len;
            return Ok(hash);
        }
        let start = Instant::now();
        let compressed = Compressor::new().compress(block_data)?;
        let comp_len: u64 = compressed.len().try_into().unwrap();
        let hex_hash = hash.to_string();
        let relpath = block_relpath(&hash);
        self.transport.create_dir(subdir_relpath(&hex_hash))?;
        increment_counter!("conserve.block.writes");
        counter!("conserve.block.write_uncompressed_bytes", uncomp_len);
        histogram!("conserve.block.write_uncompressed_bytes", uncomp_len as f64);
        counter!("conserve.block.write_compressed_bytes", comp_len);
        histogram!("conserve.block.write_compressed_bytes", comp_len as f64);
        self.transport.write_file(&relpath, &compressed)?;
        histogram!("conserve.block.compress_and_store_seconds", start.elapsed());
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
        match self.transport.metadata(&block_relpath(hash)) {
            Err(err) if err.is_not_found() => Ok(false),
            Err(err) => {
                warn!(?err, ?hash, "Error checking presence of block");
                Err(err.into())
            }
            Ok(metadata) => Ok(metadata.kind == Kind::File && metadata.len > 0),
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

    pub fn delete_block(&self, hash: &BlockHash) -> Result<()> {
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

    /// Return the entire contents of the block.
    ///
    /// Checks that the hash is correct with the contents.
    pub fn get_block_content(&self, hash: &BlockHash) -> Result<Bytes> {
        // TODO: Reuse decompressor buffer.
        // TODO: Most importantly, cache decompressed blocks!
        increment_counter!("conserve.block.read");
        let mut decompressor = Decompressor::new();
        let block_relpath = block_relpath(hash);
        let compressed_bytes = self.transport.read_file(&block_relpath)?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = BlockHash::hash_bytes(&decompressed_bytes);
        if actual_hash != *hash {
            error!(%hash, %actual_hash, %block_relpath, "Block file has wrong hash");
            return Err(Error::BlockCorrupt { hash: hash.clone() });
        }
        Ok(decompressed_bytes)
    }
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
        let mut blockdir = BlockDir::open(open_local_transport(tempdir.path()).unwrap());
        let mut stats = BackupStats::default();
        let hash = blockdir.store_or_deduplicate(b"stuff", &mut stats).unwrap();
        assert!(blockdir.contains(&hash).unwrap());
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(false)
            .open(tempdir.path().join(block_relpath(&hash)))
            .expect("Truncate block");
        assert!(!blockdir.contains(&hash).unwrap());
    }
}
