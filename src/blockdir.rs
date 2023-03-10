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
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ::metrics::{counter, histogram, increment_counter};
use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use tracing::{debug, error, info, warn};

use crate::backup::BackupStats;
use crate::blockhash::BlockHash;
use crate::compress::snappy::{Compressor, Decompressor};
use crate::kind::Kind;
use crate::progress::{Bar, Progress};
use crate::stats::Sizes;
use crate::transport::local::LocalTransport;
use crate::transport::{DirEntry, ListDirNames, Transport};
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
fn block_relpath(hash: &BlockHash) -> String {
    let hash_hex = hash.to_string();
    format!("{}/{}", subdir_relpath(&hash_hex), hash_hex)
}

impl BlockDir {
    pub fn open_path(path: &Path) -> BlockDir {
        BlockDir::open(Box::new(LocalTransport::new(path)))
    }

    pub fn open(transport: Box<dyn Transport>) -> BlockDir {
        BlockDir {
            transport: Arc::from(transport),
        }
    }

    /// Create a BlockDir directory and return an object accessing it.
    pub fn create_path(path: &Path) -> Result<BlockDir> {
        BlockDir::create(Box::new(LocalTransport::new(path)))
    }

    pub fn create(transport: Box<dyn Transport>) -> Result<BlockDir> {
        transport
            .create_dir("")
            .map_err(|source| Error::CreateBlockDir { source })?;
        Ok(BlockDir {
            transport: Arc::from(transport),
        })
    }

    /// Returns the number of compressed bytes.
    pub(crate) fn compress_and_store(&mut self, in_buf: &[u8], hash: &BlockHash) -> Result<u64> {
        // TODO: Move this to a BlockWriter, which can hold a reusable buffer.
        let mut compressor = Compressor::new();
        let uncomp_len = in_buf.len() as u64;
        let compressed = compressor.compress(in_buf)?;
        let comp_len: u64 = compressed.len().try_into().unwrap();
        let hex_hash = hash.to_string();
        let relpath = block_relpath(hash);
        self.transport.create_dir(subdir_relpath(&hex_hash))?;
        increment_counter!("conserve.block.writes");
        counter!("conserve.block.write_uncompressed_bytes", uncomp_len);
        histogram!("conserve.block.write_uncompressed_bytes", uncomp_len as f64);
        counter!("conserve.block.write_compressed_bytes", comp_len);
        histogram!("conserve.block.write_compressed_bytes", comp_len as f64);
        self.transport
            .write_file(&relpath, compressed)
            .or_else(|io_err| {
                if io_err.kind() == io::ErrorKind::AlreadyExists {
                    // Perhaps it was simultaneously created by another thread or process.
                    debug!("Unexpected late detection of existing block {hex_hash:?}");
                    Ok(())
                } else {
                    Err(Error::WriteBlock {
                        hash: hex_hash,
                        source: io_err,
                    })
                }
            })?;
        Ok(comp_len)
    }

    pub(crate) fn store_or_deduplicate(
        &mut self,
        block_data: &[u8],
        stats: &mut BackupStats,
    ) -> Result<BlockHash> {
        let hash = self.hash_bytes(block_data);
        let len = block_data.len() as u64;
        if self.contains(&hash)? {
            increment_counter!("conserve.block.matches");
            stats.deduplicated_blocks += 1;
            counter!("conserve.block.matched_bytes", len);
            stats.deduplicated_bytes += len;
        } else {
            let start = Instant::now();
            let comp_len = self.compress_and_store(block_data, &hash)?;
            histogram!("conserve.block.compress_and_store_seconds", start.elapsed());
            stats.written_blocks += 1;
            stats.uncompressed_bytes += block_data.len() as u64;
            stats.compressed_bytes += comp_len;
        }
        Ok(hash)
    }

    /// True if the named block is present in this directory.
    pub fn contains(&self, hash: &BlockHash) -> Result<bool> {
        self.transport
            .is_file(&block_relpath(hash))
            .map_err(Error::from)
    }

    /// Returns the compressed on-disk size of a block.
    pub fn compressed_size(&self, hash: &BlockHash) -> Result<u64> {
        Ok(self.transport.metadata(&block_relpath(hash))?.len)
    }

    /// Read back the contents of a block, as a byte array.
    ///
    /// To read a whole file, use StoredFile instead.
    pub fn get(&self, address: &Address) -> Result<(Vec<u8>, Sizes)> {
        let (mut decompressed, sizes) = self.get_block_content(&address.hash)?;
        let len = address.len as usize;
        let start = address.start as usize;
        let actual_len = decompressed.len();
        if (start + len) > actual_len {
            return Err(Error::AddressTooLong {
                address: address.to_owned(),
                actual_len,
            });
        }
        if start != 0 {
            let trimmed = decompressed[start..(start + len)].to_owned();
            Ok((trimmed, sizes))
        } else {
            decompressed.truncate(len);
            Ok((decompressed, sizes))
        }
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
        let ListDirNames { mut dirs, .. } = self.transport.list_dir_names("")?;
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

    fn iter_block_dir_entries(&self) -> Result<impl Iterator<Item = DirEntry>> {
        let transport = self.transport.clone();
        Ok(self
            .subdirs()?
            .into_iter()
            .map(move |subdir_name| transport.iter_dir_entries(&subdir_name))
            .filter_map(|iter_or| {
                if let Err(ref err) = iter_or {
                    error!(%err, "Error listing block subdirectory");
                }
                iter_or.ok()
            })
            .flatten()
            .filter_map(|iter_or| {
                if let Err(ref err) = iter_or {
                    error!(%err, "Error listing block subdirectory");
                }
                iter_or.ok()
            })
            .filter(|DirEntry { name, kind, .. }| {
                *kind == Kind::File
                    && name.len() == BLOCKDIR_FILE_NAME_LEN
                    && !name.starts_with(TMP_PREFIX)
            }))
    }

    /// Return all the blocknames in the blockdir, in arbitrary order.
    pub fn block_names(&self) -> Result<impl Iterator<Item = BlockHash>> {
        // TODO: Report errors
        Ok(self
            .iter_block_dir_entries()?
            .filter_map(|de| de.name.parse().ok()))
    }

    /// Return all the blocknames in the blockdir, while showing progress.
    pub fn block_names_set(&self) -> Result<HashSet<BlockHash>> {
        // TODO: We could estimate time remaining by accounting for how
        // many prefixes are present and how many have been read.
        // TODO: Read prefixes in parallel.
        let bar = Bar::new();
        Ok(self
            .iter_block_dir_entries()?
            .filter_map(|de| de.name.parse().ok())
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
                Ok((bytes, _sizes)) => {
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
    pub fn get_block_content(&self, hash: &BlockHash) -> Result<(Vec<u8>, Sizes)> {
        // TODO: Reuse decompressor buffer.
        // TODO: Reuse read buffer.
        // TODO: Most importantly, cache decompressed blocks!
        increment_counter!("conserve.block.read");
        let mut decompressor = Decompressor::new();
        let block_relpath = block_relpath(hash);
        let compressed_bytes =
            self.transport
                .read_file(&block_relpath)
                .map_err(|source| Error::ReadBlock {
                    source,
                    hash: hash.to_string(),
                })?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = BlockHash::from(blake2b::blake2b(
            BLAKE_HASH_SIZE_BYTES,
            &[],
            decompressed_bytes,
        ));
        if actual_hash != *hash {
            error!("Block file {block_relpath:?} has actual decompressed hash {actual_hash}");
            return Err(Error::BlockCorrupt {
                hash: hash.to_string(),
                actual_hash: actual_hash.to_string(),
            });
        }
        let sizes = Sizes {
            uncompressed: decompressed_bytes.len() as u64,
            compressed: compressed_bytes.len() as u64,
        };
        Ok((decompressor.take_buffer(), sizes))
    }

    fn hash_bytes(&self, in_buf: &[u8]) -> BlockHash {
        let mut hasher = Blake2b::new(BLAKE_HASH_SIZE_BYTES);
        hasher.update(in_buf);
        BlockHash::from(hasher.finalize())
    }
}
