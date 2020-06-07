// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! File contents are stored in data blocks.
//!
//! Data blocks are stored compressed, and identified by the hash of their uncompressed
//! contents.
//!
//! The contents of a file is identified by an Address, which says which block holds the data,
//! and which range of uncompressed bytes.
//!
//! The structure is: archive > blockdir > subdir > file.

use std::convert::TryInto;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::compress::snappy::{Compressor, Decompressor};
use crate::kind::Kind;
use crate::stats::{CopyStats, Sizes, ValidateBlockDirStats};
use crate::transport::local::LocalTransport;
use crate::transport::{DirEntry, TransportRead};
use crate::*;

/// Use the maximum 64-byte hash.
pub const BLAKE_HASH_SIZE_BYTES: usize = 64;

const BLOCKDIR_FILE_NAME_LEN: usize = BLAKE_HASH_SIZE_BYTES * 2;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

const TMP_PREFIX: &str = "tmp";

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
#[derive(Clone)]
pub struct BlockDir {
    pub path: PathBuf,

    transport: Box<dyn TransportRead>,
}

fn block_name_to_subdirectory(block_hash: &str) -> &str {
    &block_hash[..SUBDIR_NAME_CHARS]
}

impl BlockDir {
    /// Create a BlockDir accessing `path`, which must exist as a directory.
    pub fn new(path: &Path) -> BlockDir {
        BlockDir {
            path: path.to_path_buf(),
            transport: Box::new(LocalTransport::new(path)),
        }
    }

    /// Create a BlockDir directory and return an object accessing it.
    pub fn create(path: &Path) -> Result<BlockDir> {
        fs::create_dir(path).map_err(|source| Error::CreateBlockDir { source })?;
        Ok(BlockDir::new(path))
    }

    /// Return the subdirectory in which we'd put a file called `hash_hex`.
    fn subdir_for(&self, hash_hex: &str) -> PathBuf {
        self.path.join(block_name_to_subdirectory(hash_hex))
    }

    /// Return the full path for a file called `hex_hash`.
    fn path_for_file(&self, hash_hex: &str) -> PathBuf {
        self.subdir_for(hash_hex).join(hash_hex)
    }

    /// Return the transport-relative file for a given hash.
    fn relpath(&self, hash_hex: &str) -> String {
        format!("{}/{}", block_name_to_subdirectory(hash_hex), hash_hex)
    }

    /// Returns the number of compressed bytes.
    fn compress_and_store(&mut self, in_buf: &[u8], hex_hash: &str) -> Result<u64> {
        // TODO: Move this to a BlockWriter, which can hold a reusable buffer.
        // Note: When we come to support cloud storage, we should do one atomic write rather than
        // a write and rename.
        let path = self.path_for_file(&hex_hash);
        let d = self.subdir_for(hex_hash);
        super::io::ensure_dir_exists(&d)?;
        let mut tempf = tempfile::Builder::new()
            .prefix(TMP_PREFIX)
            .tempfile_in(&d)?;
        let mut compressor = Compressor::new();
        let compressed = compressor.compress(&in_buf)?;
        let comp_len: u64 = compressed.len().try_into().unwrap();
        tempf.write_all(compressed)?;
        // Use plain `persist` not `persist_noclobber` to avoid
        // calling `link` on Unix, which won't work on all filesystems.
        if let Err(e) = tempf.persist(&path) {
            if e.error.kind() == io::ErrorKind::AlreadyExists {
                // Perhaps it was simultaneously created by another thread or process.
                // This isn't really an error.
                ui::problem(&format!(
                    "Unexpected late detection of existing block {:?}",
                    hex_hash
                ));
                e.file.close()?;
            } else {
                return Err(e.error.into());
            }
        }
        Ok(comp_len)
    }

    /// True if the named block is present in this directory.
    pub fn contains(&self, hash: &str) -> Result<bool> {
        self.transport
            .exists(&self.relpath(hash))
            .map_err(Error::from)
    }

    /// Read back the contents of a block, as a byte array.
    ///
    /// To read a whole file, use StoredFile instead.
    pub fn get(&self, addr: &Address) -> Result<(Vec<u8>, Sizes)> {
        let (mut decompressed, sizes) = self.get_block_content(&addr.hash)?;
        let len = addr.len as usize;
        let start = addr.start as usize;
        if (start + len) > decompressed.len() {
            // TODO: Error, not panic.
            panic!(
                "address {:?} extends beyond decompressed length {}",
                addr,
                decompressed.len(),
            );
        }
        if addr.start != 0 {
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
    fn subdirs(&self) -> Result<impl Iterator<Item = String>> {
        Ok(self
            .transport
            .read_dir("")
            .map_err(|source| Error::ListBlocks {
                source,
                path: self.path.clone(),
            })?
            .filter_map(|entry_result| match entry_result {
                Err(e) => {
                    ui::problem(&format!("Error listing blockdir: {:?}", e));
                    None
                }
                Ok(DirEntry { name, kind, .. }) => {
                    if kind != Kind::Dir {
                        None
                    } else if name.len() != SUBDIR_NAME_CHARS {
                        ui::problem(&format!("Unexpected subdirectory in blockdir: {:?}", name));
                        None
                    } else {
                        Some(name)
                    }
                }
            }))
    }

    fn iter_block_dir_entries(&self) -> Result<impl Iterator<Item = DirEntry>> {
        let transport = self.transport.clone();
        Ok(self
            .subdirs()?
            .map(move |subdir_name| transport.read_dir(&subdir_name))
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
    pub fn validate(&self) -> Result<ValidateBlockDirStats> {
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
        let block_error_count = block_dir_entries
            .par_iter()
            .filter(|de| {
                ui::increment_bytes_done(de.len);
                self.get_block_content(&de.name).is_err()
            })
            .count()
            .try_into()
            .unwrap();
        let block_read_count = block_dir_entries.len().try_into().unwrap();
        Ok(ValidateBlockDirStats {
            block_error_count,
            block_read_count,
        })
    }

    /// Return the entire contents of the block.
    ///
    /// Checks that the hash is correct with the contents.
    pub fn get_block_content(&self, hash: &str) -> Result<(Vec<u8>, Sizes)> {
        // TODO: Reuse decompressor buffer.
        let mut decompressor = Decompressor::new();
        let path = self.path_for_file(hash);
        let compressed_bytes = std::fs::read(&path).map_err(|source| Error::ReadBlock {
            source,
            path: path.to_owned(),
        })?;
        let decompressed_bytes = decompressor.decompress(&compressed_bytes)?;
        let actual_hash = hex::encode(
            blake2b::blake2b(BLAKE_HASH_SIZE_BYTES, &[], &decompressed_bytes).as_bytes(),
        );
        if actual_hash != *hash {
            ui::problem(&format!(
                "Block file {:?} has actual decompressed hash {:?}",
                &path, actual_hash
            ));
            return Err(Error::BlockCorrupt { path, actual_hash });
        }
        let sizes = Sizes {
            uncompressed: decompressed_bytes.len() as u64,
            compressed: compressed_bytes.len() as u64,
        };
        // TODO: Return the existing buffer; don't copy it.
        Ok((decompressed_bytes.to_vec(), sizes))
    }

    #[allow(dead_code)]
    fn compressed_block_size(&self, hash: &str) -> Result<u64> {
        let path = self.path_for_file(hash);
        Ok(fs::metadata(&path)
            .map_err(|source| Error::ReadBlock { path, source })?
            .len())
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

    use spectral::prelude::*;
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
        let block_dir = BlockDir::new(testdir.path());
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

        // TODO: Assertions about the stats.
        let _validate_stats = block_dir.validate().unwrap();
    }

    #[test]
    pub fn retrieve_partial_data() {
        let (_testdir, block_dir) = setup();
        let mut store_files = StoreFiles::new(block_dir.clone());
        let (addrs, _stats) = store_files
            .store_file_content(
                &"/hello".into(),
                &mut io::Cursor::new("0123456789abcdef".as_bytes()),
            )
            .unwrap();
        assert_eq!(addrs.len(), 1);
        let hash = addrs[0].hash.clone();
        let first_half = Address {
            start: 0,
            len: 8,
            hash,
        };
        let (first_half_content, _first_half_stats) = block_dir.get(&first_half).unwrap();
        assert_eq!(first_half_content, "01234567".as_bytes());

        let hash = addrs[0].hash.clone();
        let second_half = Address {
            start: 8,
            len: 8,
            hash,
        };
        let (second_half_content, _second_half_stats) = block_dir.get(&second_half).unwrap();
        assert_eq!(second_half_content, "89abcdef".as_bytes());
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
        assert_that!(stats.compressed_bytes).is_less_than(MAX_BLOCK_SIZE as u64 / 10);
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
