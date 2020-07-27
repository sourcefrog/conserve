// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

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
use std::io::prelude::*;
use std::path::Path;

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::compress::snappy::{Compressor, Decompressor};
use crate::kind::Kind;
use crate::stats::{CopyStats, Sizes, ValidateStats};
use crate::transport::local::LocalTransport;
use crate::transport::{DirEntry, ListDirNames, Transport};
use crate::*;

const BLOCKDIR_FILE_NAME_LEN: usize = crate::BLAKE_HASH_SIZE_BYTES * 2;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

/// The unique identifier for a block: its hexadecimal `BLAKE2b` hash.
pub type BlockHash = String;

/// Points to some compressed data inside the block dir.
///
/// Identifiers are: which file contains it, at what (pre-compression) offset,
/// and what (pre-compression) length.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    /// ID of the block storing this info (in future, salted.)
    pub hash: String,

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
fn block_relpath(hash_hex: &str) -> String {
    format!("{}/{}", subdir_relpath(hash_hex), hash_hex)
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
    fn compress_and_store(&mut self, in_buf: &[u8], hex_hash: &str) -> Result<u64> {
        // TODO: Move this to a BlockWriter, which can hold a reusable buffer.
        let mut compressor = Compressor::new();
        let compressed = compressor.compress(&in_buf)?;
        let comp_len: u64 = compressed.len().try_into().unwrap();
        self.transport.create_dir(subdir_relpath(hex_hash))?;
        self.transport
            .write_file(&block_relpath(hex_hash), compressed)
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
                        hash: hex_hash.to_owned(),
                        source: io_err,
                    })
                }
            })?;
        Ok(comp_len)
    }

    /// True if the named block is present in this directory.
    pub fn contains(&self, hash: &str) -> Result<bool> {
        self.transport
            .exists(&block_relpath(hash))
            .map_err(Error::from)
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
    pub fn block_names(&self) -> Result<impl Iterator<Item = String>> {
        Ok(self.iter_block_dir_entries()?.map(|de| de.name))
    }

    /// Check format invariants of the BlockDir.
    ///
    /// Return a dict describing which blocks are present, and the length of their uncompressed
    /// data.
    pub fn validate(&self, stats: &mut ValidateStats) -> Result<HashMap<BlockHash, usize>> {
        // TODO: In the top-level directory, no files or directories other than prefix
        // directories of the right length.
        // TODO: Provide a progress bar that just works on counts, not bytes:
        // then we don't need to count the sizes in advance.
        // TODO: Test having a block with the right compression but the wrong contents.
        ui::println("Count blocks...");
        let block_dir_entries: Vec<DirEntry> = self.iter_block_dir_entries()?.collect();
        let tot = block_dir_entries.iter().map(|de| de.len).sum();
        ui::set_progress_phase(&"Count blocks");
        ui::set_bytes_total(tot);
        crate::ui::println(&format!(
            "Check {} in blocks...",
            crate::misc::bytes_to_human_mb(tot)
        ));
        ui::set_progress_phase(&"Check block hashes");
        // Make a vec of Some(usize) if the block could be read, or None if it failed.
        let mut results: Vec<Option<(String, usize)>> = Vec::new();
        block_dir_entries
            .par_iter()
            .map(|de| {
                ui::increment_bytes_done(de.len);
                self.get_block_content(&de.name)
                    .map(|(bytes, _sizes)| (de.name.clone(), bytes.len()))
                    .ok()
            })
            .collect_into_vec(&mut results);
        stats.block_error_count += results.iter().filter(|o| o.is_none()).count();
        let len_map: HashMap<BlockHash, usize> = results
            .into_iter()
            .filter_map(std::convert::identity)
            .collect();
        stats.block_read_count = block_dir_entries.len().try_into().unwrap();
        Ok(len_map)
    }

    /// Return the entire contents of the block.
    ///
    /// Checks that the hash is correct with the contents.
    pub fn get_block_content(&self, hash: &str) -> Result<(Vec<u8>, Sizes)> {
        // TODO: Reuse decompressor buffer.
        // TODO: Reuse read buffer.
        let mut decompressor = Decompressor::new();
        let mut compressed_bytes = Vec::new();
        let block_relpath = block_relpath(hash);
        self.transport
            .read_file(&block_relpath, &mut compressed_bytes)
            .map_err(|source| Error::ReadBlock {
                source,
                hash: hash.to_owned(),
            })?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = hex::encode(
            blake2b::blake2b(BLAKE_HASH_SIZE_BYTES, &[], &decompressed_bytes).as_bytes(),
        );
        if actual_hash != *hash {
            ui::problem(&format!(
                "Block file {:?} has actual decompressed hash {:?}",
                &block_relpath, actual_hash
            ));
            return Err(Error::BlockCorrupt {
                hash: hash.to_owned(),
                actual_hash,
            });
        }
        let sizes = Sizes {
            uncompressed: decompressed_bytes.len() as u64,
            compressed: compressed_bytes.len() as u64,
        };
        Ok((decompressor.take_buffer(), sizes))
    }
}

/// Manages storage into the BlockDir of any number of files.
///
/// At present this just holds a reusable input buffer.
///
/// In future it will combine small files into aggregate blocks,
/// and perhaps compress them in parallel.
pub(crate) struct StoreFiles {
    // TODO: Rename to FileWriter or similar? Perhaps doesn't need to be
    // separate from BackupWriter.
    block_dir: BlockDir,
    input_buf: Vec<u8>,
}

impl StoreFiles {
    pub(crate) fn new(block_dir: BlockDir) -> StoreFiles {
        StoreFiles {
            block_dir,
            input_buf: vec![0; MAX_BLOCK_SIZE],
        }
    }

    pub(crate) fn store_file_content(
        &mut self,
        apath: &Apath,
        from_file: &mut dyn Read,
    ) -> Result<(Vec<Address>, CopyStats)> {
        let mut addresses = Vec::<Address>::with_capacity(1);
        let mut stats = CopyStats::default();
        loop {
            // TODO: Possibly read repeatedly in case we get a short read and have room for more,
            // so that short reads don't lead to short blocks being stored.
            // TODO: Error should actually be an error about the source file?
            // TODO: This shouldn't directly read from the source, it should take blocks in.
            let read_len =
                from_file
                    .read(&mut self.input_buf)
                    .map_err(|source| Error::StoreFile {
                        apath: apath.to_owned(),
                        source,
                    })?;
            if read_len == 0 {
                break;
            }
            let block_data = &self.input_buf[..read_len];
            let block_hash: String = hash_bytes(block_data).unwrap();
            if self.block_dir.contains(&block_hash)? {
                // TODO: Separate counter for size of the already-present blocks?
                stats.deduplicated_blocks += 1;
                stats.deduplicated_bytes += read_len as u64;
            } else {
                let comp_len = self.block_dir.compress_and_store(block_data, &block_hash)?;
                stats.written_blocks += 1;
                stats.uncompressed_bytes += read_len as u64;
                stats.compressed_bytes += comp_len;
            }
            addresses.push(Address {
                hash: block_hash,
                start: 0,
                len: read_len as u64,
            });
        }
        match addresses.len() {
            0 => stats.empty_files += 1,
            1 => stats.single_block_files += 1,
            _ => stats.multi_block_files += 1,
        }
        Ok((addresses, stats))
    }
}

fn hash_bytes(in_buf: &[u8]) -> Result<BlockHash> {
    let mut hasher = Blake2b::new(BLAKE_HASH_SIZE_BYTES);
    hasher.update(in_buf);
    Ok(hex::encode(hasher.finalize().as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use std::io::SeekFrom;

    use tempfile::{NamedTempFile, TempDir};

    use super::*;

    const EXAMPLE_TEXT: &[u8] = b"hello!";
    const EXAMPLE_BLOCK_HASH: &str = "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd\
         3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    fn make_example_file() -> NamedTempFile {
        let mut tf = NamedTempFile::new().unwrap();
        tf.write_all(EXAMPLE_TEXT).unwrap();
        tf.flush().unwrap();
        tf.seek(SeekFrom::Start(0)).unwrap();
        tf
    }

    fn setup() -> (TempDir, BlockDir) {
        let testdir = TempDir::new().unwrap();
        let block_dir = BlockDir::create_path(testdir.path()).unwrap();
        (testdir, block_dir)
    }

    #[test]
    pub fn store_a_file() {
        let expected_hash = EXAMPLE_BLOCK_HASH.to_string();
        let (testdir, block_dir) = setup();
        let mut example_file = make_example_file();

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), false);
        let mut store = StoreFiles::new(block_dir.clone());

        let (addrs, stats) = store
            .store_file_content(&Apath::from("/hello"), &mut example_file)
            .unwrap();

        // Should be in one block, and as it's currently unsalted the hash is the same.
        assert_eq!(1, addrs.len());
        assert_eq!(0, addrs[0].start);
        assert_eq!(EXAMPLE_BLOCK_HASH, addrs[0].hash);

        // Block should be the one block present in the list.
        assert_eq!(
            block_dir.block_names().unwrap().collect::<Vec<_>>(),
            &[EXAMPLE_BLOCK_HASH]
        );

        // Subdirectory and file should exist
        let expected_file = testdir.path().join("66a").join(EXAMPLE_BLOCK_HASH);
        let attr = fs::metadata(expected_file).unwrap();
        assert!(attr.is_file());

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), true);

        assert_eq!(stats.deduplicated_blocks, 0);
        assert_eq!(stats.written_blocks, 1);
        assert_eq!(stats.uncompressed_bytes, 6);
        assert_eq!(stats.compressed_bytes, 8);

        // Will vary depending on compressor and we don't want to be too brittle.
        assert!(stats.compressed_bytes <= 19, stats.compressed_bytes);

        // Try to read back
        let (back, sizes) = block_dir.get(&addrs[0]).unwrap();
        assert_eq!(back, EXAMPLE_TEXT);
        assert_eq!(
            sizes,
            Sizes {
                uncompressed: EXAMPLE_TEXT.len() as u64,
                compressed: 8u64,
            }
        );

        let mut stats = ValidateStats::default();
        block_dir.validate(&mut stats).unwrap();
        assert_eq!(stats.io_errors, 0);
        assert_eq!(stats.block_error_count, 0);
        assert_eq!(stats.block_read_count, 1);
    }

    #[test]
    fn retrieve_partial_data() {
        let (_testdir, block_dir) = setup();
        let mut store_files = StoreFiles::new(block_dir.clone());
        let (addrs, _stats) = store_files
            .store_file_content(&"/hello".into(), &mut io::Cursor::new(b"0123456789abcdef"))
            .unwrap();
        assert_eq!(addrs.len(), 1);
        let hash = addrs[0].hash.clone();
        let first_half = Address {
            start: 0,
            len: 8,
            hash,
        };
        let (first_half_content, _first_half_stats) = block_dir.get(&first_half).unwrap();
        assert_eq!(first_half_content, b"01234567");

        let hash = addrs[0].hash.clone();
        let second_half = Address {
            start: 8,
            len: 8,
            hash,
        };
        let (second_half_content, _second_half_stats) = block_dir.get(&second_half).unwrap();
        assert_eq!(second_half_content, b"89abcdef");
    }

    #[test]
    fn invalid_addresses() {
        let (_testdir, block_dir) = setup();
        let mut store_files = StoreFiles::new(block_dir.clone());
        let (addrs, _stats) = store_files
            .store_file_content(&"/hello".into(), &mut io::Cursor::new(b"0123456789abcdef"))
            .unwrap();
        assert_eq!(addrs.len(), 1);

        // Address with start point too high.
        let hash = addrs[0].hash.clone();
        let starts_too_late = Address {
            hash: hash.clone(),
            start: 16,
            len: 2,
        };
        let result = block_dir.get(&starts_too_late);
        assert_eq!(
            &result.err().unwrap().to_string(),
            &format!(
                "Address {{ hash: {:?}, start: 16, len: 2 }} \
                   extends beyond decompressed block length 16",
                hash
            )
        );

        // Address with length too long.
        let too_long = Address {
            hash: hash.clone(),
            start: 10,
            len: 10,
        };
        let result = block_dir.get(&too_long);
        assert_eq!(
            &result.err().unwrap().to_string(),
            &format!(
                "Address {{ hash: {:?}, start: 10, len: 10 }} \
                   extends beyond decompressed block length 16",
                hash
            )
        );
    }

    #[test]
    pub fn write_same_data_again() {
        let (_testdir, block_dir) = setup();

        let mut example_file = make_example_file();
        let mut store = StoreFiles::new(block_dir);
        let (addrs1, stats) = store
            .store_file_content(&Apath::from("/ello"), &mut example_file)
            .unwrap();
        assert_eq!(stats.deduplicated_blocks, 0);
        assert_eq!(stats.written_blocks, 1);
        assert_eq!(stats.uncompressed_bytes, 6);
        assert_eq!(stats.compressed_bytes, 8);

        let mut example_file = make_example_file();
        let (addrs2, stats2) = store
            .store_file_content(&Apath::from("/ello2"), &mut example_file)
            .unwrap();
        assert_eq!(stats2.deduplicated_blocks, 1);
        assert_eq!(stats2.written_blocks, 0);
        assert_eq!(stats2.compressed_bytes, 0);

        assert_eq!(addrs1, addrs2);
    }

    #[test]
    // Large enough that it should break across blocks.
    pub fn large_file() {
        use super::MAX_BLOCK_SIZE;
        let (_testdir, block_dir) = setup();
        let mut tf = NamedTempFile::new().unwrap();
        const N_CHUNKS: u64 = 10;
        const CHUNK_SIZE: u64 = 1 << 21;
        const TOTAL_SIZE: u64 = N_CHUNKS * CHUNK_SIZE;
        let a_chunk = vec![b'@'; CHUNK_SIZE as usize];
        for _i in 0..N_CHUNKS {
            tf.write_all(&a_chunk).unwrap();
        }
        tf.flush().unwrap();
        let tf_len = tf.seek(SeekFrom::Current(0)).unwrap();
        println!("tf len={}", tf_len);
        assert_eq!(tf_len, TOTAL_SIZE);
        tf.seek(SeekFrom::Start(0)).unwrap();

        let mut store = StoreFiles::new(block_dir.clone());
        let (addrs, stats) = store
            .store_file_content(&Apath::from("/big"), &mut tf)
            .unwrap();

        // Only one block needs to get compressed. The others are deduplicated.
        assert_eq!(stats.uncompressed_bytes, MAX_BLOCK_SIZE as u64);
        // Should be very compressible
        assert!(stats.compressed_bytes < (MAX_BLOCK_SIZE as u64 / 10));
        assert_eq!(stats.written_blocks, 1);
        assert_eq!(
            stats.deduplicated_blocks as u64,
            TOTAL_SIZE / (MAX_BLOCK_SIZE as u64) - 1
        );

        // 10x 2MB should be twenty blocks
        assert_eq!(addrs.len(), 20);
        for a in addrs {
            let (retr, block_sizes) = block_dir.get(&a).unwrap();
            assert_eq!(retr.len(), MAX_BLOCK_SIZE as usize);
            assert!(retr.iter().all(|b| *b == 64u8));
            assert_eq!(block_sizes.uncompressed, MAX_BLOCK_SIZE as u64);
        }
    }
}
