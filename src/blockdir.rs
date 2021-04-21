// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2021 Martin Pool.

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
use std::convert::TryInto;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use thousands::Separable;

use crate::blockhash::BlockHash;
use crate::compress::snappy::{Compressor, Decompressor};
use crate::kind::Kind;
use crate::stats::{BackupStats, Sizes, ValidateStats};
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
    transport: Box<dyn Transport>,
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
        BlockDir { transport }
    }

    /// Create a BlockDir directory and return an object accessing it.
    pub fn create_path(path: &Path) -> Result<BlockDir> {
        BlockDir::create(Box::new(LocalTransport::new(path)))
    }

    pub fn create(transport: Box<dyn Transport>) -> Result<BlockDir> {
        transport
            .create_dir("")
            .map_err(|source| Error::CreateBlockDir { source })?;
        Ok(BlockDir { transport })
    }

    /// Returns the number of compressed bytes.
    pub(crate) fn compress_and_store(&mut self, in_buf: &[u8], hash: &BlockHash) -> Result<u64> {
        // TODO: Move this to a BlockWriter, which can hold a reusable buffer.
        let mut compressor = Compressor::new();
        let compressed = compressor.compress(&in_buf)?;
        let comp_len: u64 = compressed.len().try_into().unwrap();
        let hex_hash = hash.to_string();
        let relpath = block_relpath(hash);
        self.transport.create_dir(subdir_relpath(&hex_hash))?;
        self.transport
            .write_file(&relpath, compressed)
            .or_else(|io_err| {
                if io_err.kind() == io::ErrorKind::AlreadyExists {
                    // Perhaps it was simultaneously created by another thread or process.
                    ui::problem(&format!(
                        "Unexpected late detection of existing block {:?}",
                        hex_hash
                    ));
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
        if self.contains(&hash)? {
            stats.deduplicated_blocks += 1;
            stats.deduplicated_bytes += block_data.len() as u64;
        } else {
            let comp_len = self.compress_and_store(block_data, &hash)?;
            stats.written_blocks += 1;
            stats.uncompressed_bytes += block_data.len() as u64;
            stats.compressed_bytes += comp_len;
        }
        Ok(hash)
    }

    /// True if the named block is present in this directory.
    pub fn contains(&self, hash: &BlockHash) -> Result<bool> {
        self.transport
            .exists(&block_relpath(hash))
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
                ui::problem(&format!(
                    "Unexpected subdirectory in blockdir: {:?}",
                    dirname
                ));
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
                    ui::problem(&format!("Error listing block directory: {:?}", &err));
                }
                iter_or.ok()
            })
            .flatten()
            .filter_map(|iter_or| {
                if let Err(ref err) = iter_or {
                    ui::problem(&format!("Error listing block subdirectory: {:?}", &err));
                }
                iter_or.ok()
            })
            .filter(|DirEntry { name, kind, .. }| {
                *kind == Kind::File
                    && name.len() == BLOCKDIR_FILE_NAME_LEN
                    && !name.starts_with(TMP_PREFIX)
            }))
    }

    /// Return an iterator through all the blocknames in the blockdir,
    /// in arbitrary order.
    pub fn block_names(&self) -> Result<impl Iterator<Item = BlockHash>> {
        Ok(self
            .iter_block_dir_entries()?
            .filter_map(|de| de.name.parse().ok()))
    }

    /// Check format invariants of the BlockDir.
    ///
    /// Return a dict describing which blocks are present, and the length of their uncompressed
    /// data.
    pub fn validate(&self, stats: &mut ValidateStats) -> Result<HashMap<BlockHash, usize>> {
        // TODO: In the top-level directory, no files or directories other than prefix
        // directories of the right length.
        // TODO: Test having a block with the right compression but the wrong contents.
        ui::println("Count blocks...");
        let mut progress_bar = ProgressBar::new();
        progress_bar.set_phase("Count blocks");
        let blocks: Vec<BlockHash> = self
            .block_names()?
            .enumerate()
            .inspect(|(i, _hash)| {
                if i % 100 == 0 {
                    progress_bar.set_work_done(*i)
                }
            })
            .map(|(_i, hash)| hash)
            .collect();
        crate::ui::println(&format!(
            "Check {} blocks...",
            blocks.len().separate_with_commas()
        ));
        progress_bar.set_total_work(blocks.len());
        stats.block_read_count = blocks.len().try_into().unwrap();
        let block_count = blocks.len();
        progress_bar.set_phase("Check block hashes");
        progress_bar.set_total_work(block_count);
        progress_bar.set_work_done(0);
        let pb_mutex = Mutex::new(progress_bar);
        // Make a vec of Some(usize) if the block could be read, or None if it
        // failed, where the usize gives the uncompressed data size.
        let mut results: Vec<Option<(BlockHash, usize)>> = Vec::new();
        blocks
            .into_par_iter()
            .map(|hash| {
                let r = self
                    .get_block_content(&hash)
                    .map(|(bytes, _sizes)| (hash, bytes.len()))
                    .ok();
                let mut pbl = pb_mutex.lock().unwrap();
                pbl.increment_work_done(1);
                if let Some(ref t) = r {
                    pbl.increment_bytes_done(t.1 as u64);
                }
                r
            })
            .collect_into_vec(&mut results);
        stats.block_error_count += results.iter().filter(|o| o.is_none()).count();
        let len_map: HashMap<BlockHash, usize> = results
            .into_iter()
            .flatten() // keep only Some values
            .collect();
        Ok(len_map)
    }

    /// Return the entire contents of the block.
    ///
    /// Checks that the hash is correct with the contents.
    pub fn get_block_content(&self, hash: &BlockHash) -> Result<(Vec<u8>, Sizes)> {
        // TODO: Reuse decompressor buffer.
        // TODO: Reuse read buffer.
        let mut decompressor = Decompressor::new();
        let mut compressed_bytes = Vec::new();
        let block_relpath = block_relpath(hash);
        self.transport
            .read_file(&block_relpath, &mut compressed_bytes)
            .map_err(|source| Error::ReadBlock {
                source,
                hash: hash.to_string(),
            })?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = BlockHash::from(blake2b::blake2b(
            BLAKE_HASH_SIZE_BYTES,
            &[],
            &decompressed_bytes,
        ));
        if actual_hash != *hash {
            ui::problem(&format!(
                "Block file {:?} has actual decompressed hash {}",
                &block_relpath, actual_hash
            ));
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
