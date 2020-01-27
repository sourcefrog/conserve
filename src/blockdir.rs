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

use std::fs;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use tempfile;

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

    fn compress_and_store(
        &self,
        in_buf: &[u8],
        hex_hash: &str,
        report: &Report,
    ) -> std::io::Result<u64> {
        // Note: When we come to support cloud storage, we should do one atomic write rather than
        // a write and rename.
        let path = self.path_for_file(&hex_hash);
        let d = self.subdir_for(hex_hash);
        super::io::ensure_dir_exists(&d)?;
        let tempf = tempfile::Builder::new()
            .prefix(TMP_PREFIX)
            .tempfile_in(&d)?;
        let mut bufw = io::BufWriter::new(tempf);
        Snappy::compress_and_write(&in_buf, &mut bufw)?;
        let tempf = bufw.into_inner().unwrap();

        // TODO: Count bytes rather than stat-ing.
        let comp_len = tempf.as_file().metadata()?.len();

        // Also use plain `persist` not `persist_noclobber` to avoid
        // calling `link` on Unix, which won't work on all filesystems.
        if let Err(e) = tempf.persist(&path) {
            if e.error.kind() == io::ErrorKind::AlreadyExists {
                // Suprising we saw this rather than detecting it above.
                report.problem(&format!(
                    "Unexpected late detection of existing block {:?}",
                    hex_hash
                ));
                report.increment("block.already_present", 1);
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
    pub fn get(&self, addr: &Address, report: &Report) -> Result<Vec<u8>> {
        if addr.start != 0 {
            unimplemented!();
        }
        let decompressed = self.get_block_content(&addr.hash, report)?;
        // TODO: Accept addresses referring to only part of a block.
        if decompressed.len() != addr.len as usize {
            unimplemented!();
        }
        Ok(decompressed)
    }

    /// Return a sorted vec of prefix subdirectories.
    fn subdirs(&self, report: &Report) -> std::io::Result<Vec<String>> {
        // This doesn't check every invariant that should be true; that's the job of the validation
        // code.
        let (_fs, mut ds) = list_dir(&self.path)?;
        ds.retain(|dd| {
            if dd.len() != SUBDIR_NAME_CHARS {
                report.problem(&format!(
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

    fn iter_block_dir_entries(
        &self,
        report: &Report,
    ) -> Result<impl Iterator<Item = std::fs::DirEntry>> {
        let path = self.path.clone();
        let subdirs = self
            .subdirs(report)
            .with_context(|| errors::ListBlocks { path: path.clone() })?;
        Ok(subdirs.into_iter().flat_map(move |s| {
            // TODO: Avoid `unwrap`; send errors to the report.
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
    pub fn block_names(&self, report: &Report) -> Result<impl Iterator<Item = String>> {
        Ok(self
            .iter_block_dir_entries(report)?
            .map(|de| de.file_name().into_string().unwrap()))
    }

    /// Return an iterator of block names and sizes.
    fn block_names_and_sizes(
        &self,
        report: &Report,
    ) -> Result<impl Iterator<Item = (String, u64)>> {
        Ok(self.iter_block_dir_entries(report)?.map(|de| {
            (
                de.file_name().into_string().unwrap(),
                de.metadata().unwrap().len(),
            )
        }))
    }

    /// Check format invariants of the BlockDir; report any problems to the Report.
    pub fn validate(&self, report: &Report) -> Result<()> {
        // TODO: In the top-level directory, no files or directories other than prefix
        // directories of the right length.
        // TODO: Provide a progress bar that just works on counts, not bytes:
        // then we don't need to count the sizes in advance.
        report.set_phase("Count blocks");
        report.print("Count blocks...");
        let bns: Vec<(String, u64)> = self.block_names_and_sizes(report)?.collect();
        let tot = bns.iter().map(|a| a.1).sum();
        report.set_total_work(tot);
        report.print(&format!(
            "Check {} in blocks...",
            crate::misc::bytes_to_human_mb(tot)
        ));
        report.set_phase("Check block hashes");
        bns.par_iter()
            .map(|(bn, bsize)| {
                report.increment_work(*bsize);
                self.validate_block(bn, report)
            })
            .try_for_each(|i| i)?;
        Ok(())
    }

    fn validate_block(&self, hash: &str, report: &Report) -> Result<()> {
        let de = self.get_block_content(&hash, report)?;
        let actual_hash = hex::encode(blake2b::blake2b(BLAKE_HASH_SIZE_BYTES, &[], &de).as_bytes());
        if actual_hash != *hash {
            let path = self.path_for_file(&hash);
            report.increment("block.misplaced", 1);
            report.problem(&format!(
                "Block file {:?} has actual decompressed hash {:?}",
                path, actual_hash
            ));
            return Err(Error::BlockCorrupt { path, actual_hash });
        }
        Ok(())
    }

    /// Return the entire contents of the block.
    pub fn get_block_content(&self, hash: &str, report: &Report) -> Result<Vec<u8>> {
        // TODO: Probably this should return an iterator rather than pulling the
        // whole file in to memory immediately.
        let path = self.path_for_file(hash);
        let (compressed_len, de) = File::open(&path)
            .and_then(|mut f| Snappy::decompress_read(&mut f))
            .context(errors::ReadBlock { path })
            .map_err(|e| {
                report.increment("block.corrupt", 1);
                report.show_error(&e);
                e
            })?;
        report.increment("block.read", 1);
        report.increment_size(
            "block",
            Sizes {
                uncompressed: de.len() as u64,
                compressed: compressed_len as u64,
            },
        );
        Ok(de)
    }

    #[allow(dead_code)]
    fn compressed_block_size(&self, hash: &str) -> Result<u64> {
        let path = self.path_for_file(hash);
        Ok(fs::metadata(&path)
            .context(errors::ReadBlock { path })?
            .len())
    }

    pub(crate) fn contains_all_blocks(&self, addrs: &[Address]) -> bool {
        addrs
            .iter()
            .all(|a| self.contains(&a.hash).unwrap_or_default())
    }
}

/// Manages storage into the BlockDir of any number of files.
///
/// At present this just holds a reusable input buffer.
///
/// In future it will combine small files into aggregate blocks,
/// and perhaps compress them in parallel.
pub(crate) struct StoreFiles {
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
        report: &Report,
    ) -> Result<Vec<Address>> {
        let mut addresses = Vec::<Address>::with_capacity(1);
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
            let block_data = &self.input_buf[..read_len];
            let block_hash: String = hash_bytes(block_data).unwrap();
            if self.block_dir.contains(&block_hash)? {
                report.increment("block.already_present", 1);
            } else {
                let comp_len = self
                    .block_dir
                    .compress_and_store(block_data, &block_hash, &report)
                    .with_context(|| errors::StoreBlock {
                        block_hash: block_hash.clone(),
                    })?;
                report.increment("block.write", 1);
                report.increment_size(
                    "block",
                    Sizes {
                        compressed: comp_len,
                        uncompressed: read_len as u64,
                    },
                );
            }
            addresses.push(Address {
                hash: block_hash,
                start: 0,
                len: read_len as u64,
            });
        }
        match addresses.len() {
            0 => report.increment("file.empty", 1),
            1 => report.increment("file.medium", 1),
            _ => report.increment("file.large", 1),
        }
        Ok(addresses)
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
        let report = Report::new();
        let (testdir, block_dir) = setup();
        let mut example_file = make_example_file();

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), false);
        let mut store = StoreFiles::new(block_dir.clone());

        let addrs = store
            .store_file_content(&Apath::from("/hello"), &mut example_file, &report)
            .unwrap();

        // Should be in one block, and as it's currently unsalted the hash is the same.
        assert_eq!(1, addrs.len());
        assert_eq!(0, addrs[0].start);
        assert_eq!(EXAMPLE_BLOCK_HASH, addrs[0].hash);

        // Block should be the one block present in the list.
        assert_eq!(
            block_dir
                .block_names(&Report::new())
                .unwrap()
                .collect::<Vec<_>>(),
            &[EXAMPLE_BLOCK_HASH]
        );

        // Subdirectory and file should exist
        let expected_file = testdir.path().join("66a").join(EXAMPLE_BLOCK_HASH);
        let attr = fs::metadata(expected_file).unwrap();
        assert!(attr.is_file());

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), true);

        assert_eq!(report.get_count("block.already_present"), 0);
        assert_eq!(report.get_count("block.write"), 1);
        let sizes = report.get_size("block");
        assert_eq!(sizes.uncompressed, 6);

        // Will vary depending on compressor and we don't want to be too brittle.
        assert!(sizes.compressed <= 19, sizes.compressed);

        // Try to read back
        let read_report = Report::new();
        assert_eq!(read_report.get_count("block.read"), 0);
        let back = block_dir.get(&addrs[0], &read_report).unwrap();
        assert_eq!(back, EXAMPLE_TEXT);
        assert_eq!(read_report.get_count("block.read"), 1);
        assert_eq!(
            read_report.get_size("block"),
            Sizes {
                uncompressed: EXAMPLE_TEXT.len() as u64,
                compressed: 8u64,
            }
        );

        // Validate
        let validate_r = Report::new();
        block_dir.validate(&validate_r).unwrap();
    }

    #[test]
    pub fn write_same_data_again() {
        let report = Report::new();
        let (_testdir, block_dir) = setup();

        let mut example_file = make_example_file();
        let mut store = StoreFiles::new(block_dir);
        let addrs1 = store
            .store_file_content(&Apath::from("/ello"), &mut example_file, &report)
            .unwrap();
        assert_eq!(report.get_count("block.already_present"), 0);
        assert_eq!(report.get_count("block.write"), 1);

        let mut example_file = make_example_file();
        let addrs2 = store
            .store_file_content(&Apath::from("/ello2"), &mut example_file, &report)
            .unwrap();
        assert_eq!(report.get_count("block.already_present"), 1);
        assert_eq!(report.get_count("block.write"), 1);

        assert_eq!(addrs1, addrs2);
    }

    #[test]
    // Large enough that it should break across blocks.
    pub fn large_file() {
        use super::MAX_BLOCK_SIZE;
        let report = Report::new();
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
        let addrs = store
            .store_file_content(&Apath::from("/big"), &mut tf, &report)
            .unwrap();
        println!("Report after store: {}", report);

        // Since the blocks are identical we should see them only stored once, and several
        // blocks repeated.
        assert_eq!(report.get_size("block").uncompressed, MAX_BLOCK_SIZE as u64);
        // Should be very compressible
        assert!(report.get_size("block").compressed < (MAX_BLOCK_SIZE as u64 / 10));
        assert_eq!(report.get_count("block.write"), 1);
        assert_eq!(
            report.get_count("block.already_present"),
            TOTAL_SIZE / (MAX_BLOCK_SIZE as u64) - 1
        );

        // 10x 2MB should be twenty blocks
        assert_eq!(addrs.len(), 20);
        for a in addrs {
            let retr = block_dir.get(&a, &report).unwrap();
            assert_eq!(retr.len(), MAX_BLOCK_SIZE as usize);
            assert!(retr.iter().all(|b| *b == 64u8));
        }
    }
}
