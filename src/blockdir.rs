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
use snafu::ResultExt;

use crate::compress::snappy;
use crate::stats::CopyStats;
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
#[derive(Clone, Debug)]
pub struct BlockDir {
    pub path: PathBuf,
}

fn block_name_to_subdirectory(block_hash: &str) -> &str {
    &block_hash[..SUBDIR_NAME_CHARS]
}

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct ValidateBlockDirStats {
    /// Number of blocks read.
    pub block_read_count: u64,
    /// Number of blocks that failed to read back.
    pub block_error_count: u64,
}

impl BlockDir {
    /// Create a BlockDir accessing `path`, which must exist as a directory.
    pub fn new(path: &Path) -> BlockDir {
        BlockDir {
            path: path.to_path_buf(),
        }
    }

    /// Create a BlockDir directory and return an object accessing it.
    pub fn create(path: &Path) -> Result<BlockDir> {
        fs::create_dir(path).context(errors::CreateBlockDir)?;
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

    fn compress_and_store(&self, in_buf: &[u8], hex_hash: &str) -> std::io::Result<u64> {
        // Note: When we come to support cloud storage, we should do one atomic write rather than
        // a write and rename.
        let path = self.path_for_file(&hex_hash);
        let d = self.subdir_for(hex_hash);
        super::io::ensure_dir_exists(&d)?;
        let mut tempf = tempfile::Builder::new()
            .prefix(TMP_PREFIX)
            .tempfile_in(&d)?;
        let comp_len = Snappy::compress_and_write(&in_buf, &mut tempf)?
            .try_into()
            .unwrap();
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
                return Err(e.error);
            }
        }
        Ok(comp_len)
    }

    /// True if the named block is present in this directory.
    pub fn contains(&self, hash: &str) -> Result<bool> {
        let path = self.path_for_file(hash);
        match fs::metadata(&path) {
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Ok(_) => Ok(true),
            Err(e) => Err(e).context(errors::ReadBlock { path }),
        }
    }

    /// Read back the contents of a block, as a byte array.
    ///
    /// To read a whole file, use StoredFile instead.
    pub fn get(&self, addr: &Address) -> Result<(Vec<u8>, Sizes)> {
        if addr.start != 0 {
            todo!("Reading parts of blocks is not supported (or expected) yet");
        }
        let (decompressed, sizes) = self.get_block_content(&addr.hash)?;
        // TODO: Accept addresses referring to only part of a block.
        if decompressed.len() != addr.len as usize {
            todo!("Reading parts of blocks is not supported (or expected) yet");
        }
        Ok((decompressed, sizes))
    }

    /// Return a sorted vec of prefix subdirectories.
    fn subdirs(&self) -> std::io::Result<Vec<String>> {
        // This doesn't check every invariant that should be true; that's the job of the validation
        // code.
        let (_fs, mut ds) = list_dir(&self.path)?;
        ds.retain(|dd| {
            if dd.len() != SUBDIR_NAME_CHARS {
                ui::problem(&format!(
                    "unexpected subdirectory in blockdir {:?}: {:?}",
                    self, dd
                ));
                false
            } else {
                true
            }
        });
        Ok(ds)
    }

    fn iter_block_dir_entries(&self) -> Result<impl Iterator<Item = std::fs::DirEntry>> {
        let path = self.path.clone();
        let subdirs = self
            .subdirs()
            .with_context(|| errors::ListBlocks { path: path.clone() })?;
        Ok(subdirs.into_iter().flat_map(move |s| {
            // TODO: Avoid `unwrap`.
            fs::read_dir(&path.join(s))
                .unwrap()
                .map(std::io::Result::unwrap)
                .filter(|entry| {
                    let name = entry.file_name().into_string().unwrap();
                    entry.file_type().unwrap().is_file()
                        && !name.starts_with(TMP_PREFIX)
                        && name.len() == BLOCKDIR_FILE_NAME_LEN
                })
        }))
    }

    /// Return an iterator through all the blocknames in the blockdir,
    /// in arbitrary order.
    pub fn block_names(&self) -> Result<impl Iterator<Item = String>> {
        Ok(self
            .iter_block_dir_entries()?
            .map(|de| de.file_name().into_string().unwrap()))
    }

    /// Return an iterator of block names and sizes.
    fn block_names_and_sizes(&self) -> Result<impl Iterator<Item = (String, u64)>> {
        Ok(self.iter_block_dir_entries()?.map(|de| {
            (
                de.file_name().into_string().unwrap(),
                de.metadata().unwrap().len(),
            )
        }))
    }

    /// Check format invariants of the BlockDir.
    pub fn validate(&self) -> Result<ValidateBlockDirStats> {
        // TODO: In the top-level directory, no files or directories other than prefix
        // directories of the right length.
        // TODO: Provide a progress bar that just works on counts, not bytes:
        // then we don't need to count the sizes in advance.
        ui::println("Count blocks...");
        let bns: Vec<(String, u64)> = self.block_names_and_sizes()?.collect();
        let tot = bns.iter().map(|a| a.1).sum();
        ui::set_progress_phase(&"Count blocks");
        ui::set_bytes_total(tot);
        crate::ui::println(&format!(
            "Check {} in blocks...",
            crate::misc::bytes_to_human_mb(tot)
        ));
        ui::set_progress_phase(&"Check block hashes");
        // TODO: Accumulate counts from validation of individual blocks,
        // and count the total number that were unreadable or had the wrong hash.
        let block_error_count = bns
            .par_iter()
            .filter(|(block_hash, bsize)| {
                ui::increment_bytes_done(*bsize);
                self.get_block_content(&block_hash).is_err()
            })
            .count()
            .try_into()
            .unwrap();
        let block_read_count = bns.len().try_into().unwrap();
        Ok(ValidateBlockDirStats {
            block_error_count,
            block_read_count,
        })
    }

    /// Return the entire contents of the block.
    pub fn get_block_content(&self, hash: &str) -> Result<(Vec<u8>, Sizes)> {
        let path = self.path_for_file(hash);
        let (compressed_len, decompressed_bytes) = snappy::decompress_file(&path)
            .context(errors::ReadBlock { path: path.clone() })
            .map_err(|e| {
                ui::show_error(&e);
                e
            })?;
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
            compressed: compressed_len as u64,
        };
        Ok((decompressed_bytes, sizes))
    }

    #[allow(dead_code)]
    fn compressed_block_size(&self, hash: &str) -> Result<u64> {
        let path = self.path_for_file(hash);
        Ok(fs::metadata(&path)
            .context(errors::ReadBlock { path })?
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
            let read_len =
                from_file
                    .read(&mut self.input_buf)
                    .with_context(|| errors::StoreFile {
                        apath: apath.clone(),
                    })?;
            if read_len == 0 {
                break;
            }
            stats.uncompressed_bytes += read_len as u64;
            let block_data = &self.input_buf[..read_len];
            let block_hash: String = hash_bytes(block_data).unwrap();
            if self.block_dir.contains(&block_hash)? {
                // TODO: Separate counter for size of the already-present blocks?
                stats.deduplicated_blocks += 1;
                stats.deduplicated_bytes += read_len as u64;
            } else {
                let comp_len = self
                    .block_dir
                    .compress_and_store(block_data, &block_hash)
                    .with_context(|| errors::StoreBlock {
                        block_hash: block_hash.clone(),
                    })?;
                stats.written_blocks += 1;
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

        assert_eq!(stats.uncompressed_bytes, TOTAL_SIZE);
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
