// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! File contents are stored in data blocks.
//!
//! Data blocks are stored compressed, and identified by the hash of their uncompressed
//! contents.
//!
//! The contents of a file is identified by an Address, which says which block holds the data,
//! and which range of uncompressed bytes.
//!
//! The structure is: archive > band > blockdir > subdir > file.

use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use rustc_serialize::hex::ToHex;

use tempfile;

use super::*;

/// Use the maximum 64-byte hash.
const BLAKE_HASH_SIZE_BYTES: usize = 64;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

/// Break blocks at this many uncompressed bytes.
const MAX_BLOCK_SIZE: usize = 1 << 20;

/// The unique identifier for a block: its hexadecimal `BLAKE2b` hash.
pub type BlockHash = String;

/// Points to some compressed data inside the block dir.
///
/// Identifiers are: which file contains it, at what (pre-compression) offset,
/// and what (pre-compression) length.
#[derive(Clone, Debug, PartialEq, RustcDecodable, RustcEncodable)]
pub struct Address {
    /// ID of the block storing this info (in future, salted.)
    pub hash: String,

    /// Position in this block where data begins.
    pub start: u64,

    /// Length of this block to be used.
    pub len: u64,
}

/// A readable, writable directory within a band holding data blocks.
#[derive(Debug)]
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

    /// Return the subdirectory in which we'd put a file called `hash_hex`.
    fn subdir_for(self: &BlockDir, hash_hex: &str) -> PathBuf {
        let mut buf = self.path.clone();
        buf.push(block_name_to_subdirectory(hash_hex));
        buf
    }

    /// Return the full path for a file called `hex_hash`.
    fn path_for_file(self: &BlockDir, hash_hex: &str) -> PathBuf {
        let mut buf = self.subdir_for(hash_hex);
        buf.push(hash_hex);
        buf
    }

    /// Store the contents of a readable file into the BlockDir.
    ///
    /// Returns the addresses at which it was stored, plus the hash of the overall original file.
    pub fn store(
        &mut self,
        from_file: &mut Read,
        report: &Report,
    ) -> Result<(Vec<Address>, BlockHash)> {
        // TODO: Split large files, combine small files. Don't read them all into a single buffer.

        // loop
        //   read up to block_size bytes
        //   accumulate into the overall hasher
        //   hash those bytes - as a special case if this is the first block, it's the same as
        //     the overall hash.
        //   if already stored: don't store again
        //   compress and store
        let mut addresses = Vec::<Address>::with_capacity(1);
        let mut file_hasher = Blake2b::new(BLAKE_HASH_SIZE_BYTES);
        let mut in_buf = Vec::<u8>::with_capacity(MAX_BLOCK_SIZE);
        loop {
            unsafe {
                // Increase size to capacity without initializing data that will be overwritten.
                in_buf.set_len(MAX_BLOCK_SIZE);
            };
            // TODO: Possibly read repeatedly in case we get a short read and have room for more,
            // so that short reads don't lead to short blocks being stored.
            let read_len =
                report.measure_duration("source.read", || from_file.read(&mut in_buf))?;
            if read_len == 0 {
                break;
            }
            in_buf.truncate(read_len);

            let block_hash: String;
            if addresses.is_empty() {
                report.measure_duration("file.hash", || file_hasher.update(&in_buf));
                block_hash = file_hasher.clone().finalize().as_bytes().to_hex()
            } else {
                // Not the first block, must update file and block hash separately, but we can do
                // them in parallel.
                block_hash = rayon::join(
                    || report.measure_duration("file.hash", || file_hasher.update(&in_buf)),
                    || report.measure_duration("block.hash", || hash_bytes(&in_buf).unwrap()),
                ).1;
            }

            if self.contains(&block_hash)? {
                report.increment("block.already_present", 1);
            } else {
                let comp_len = self.compress_and_store(&in_buf, &block_hash, &report)?;
                // Maybe rename counter to 'block.write'?
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
                hash: block_hash.clone(),
                start: 0,
                len: read_len as u64,
            });
        }
        match addresses.len() {
            0 => report.increment("file.empty", 1),
            1 => report.increment("file.medium", 1),
            _ => report.increment("file.large", 1),
        }
        Ok((addresses, file_hasher.finalize().as_bytes().to_hex()))
    }

    fn compress_and_store(&self, in_buf: &[u8], hex_hash: &str, report: &Report) -> Result<u64> {
        super::io::ensure_dir_exists(&self.subdir_for(hex_hash))?;
        let tempf = tempfile::NamedTempFileOptions::new()
            .prefix("tmp")
            .create_in(&self.path)?;
        let mut bufw = io::BufWriter::new(tempf);
        report.measure_duration("block.compress", || {
            Snappy::compress_and_write(&in_buf, &mut bufw)
        })?;
        let tempf = bufw.into_inner().unwrap();
        // report.measure_duration("sync", || tempf.sync_all())?;

        // TODO: Count bytes rather than stat-ing.
        let comp_len = tempf.metadata()?.len();

        // Also use plain `persist` not `persist_noclobber` to avoid
        // calling `link` on Unix, which won't work on all filesystems.
        if let Err(e) = tempf.persist(&self.path_for_file(&hex_hash)) {
            if e.error.kind() == io::ErrorKind::AlreadyExists {
                // Suprising we saw this rather than detecting it above.
                warn!("Unexpected late detection of existing block {:?}", hex_hash);
                report.increment("block.already_present", 1);
            } else {
                return Err(e.error.into());
            }
        }
        Ok(comp_len)
    }

    /// True if the named block is present in this directory.
    pub fn contains(self: &BlockDir, hash: &str) -> Result<bool> {
        match fs::metadata(self.path_for_file(hash)) {
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Ok(_) => Ok(true),
            Err(e) => Err(e.into()),
        }
    }

    /// Read back the contents of a block, as a byte array.
    ///
    /// To read a whole file, use StoredFile instead.
    pub fn get(self: &BlockDir, addr: &Address, report: &Report) -> Result<Vec<u8>> {
        // TODO: Return a Read rather than a Vec?
        // TODO: Accept vectors of multiple addresess, maybe in another function.
        let hash = &addr.hash;
        if addr.start != 0 {
            unimplemented!();
        }
        let path = self.path_for_file(hash);
        let mut f = fs::File::open(&path)?;

        // TODO: Specific error for compression failure (corruption?) vs io errors.
        let (compressed_len, decompressed) = match Snappy::decompress_read(&mut f) {
            Ok(d) => d,
            Err(e) => {
                report.increment("block.corrupt", 1);
                error!("Block file {:?} read error {:?}", path, e);
                return Err(ErrorKind::BlockCorrupt(hash.clone()).into());
            }
        };
        // TODO: Accept addresses referring to only part of a block.
        if decompressed.len() != addr.len as usize {
            unimplemented!();
        }
        report.increment("block.read", 1);
        report.increment_size(
            "block",
            Sizes {
                uncompressed: decompressed.len() as u64,
                compressed: compressed_len as u64,
            },
        );

        let actual_hash = blake2b::blake2b(BLAKE_HASH_SIZE_BYTES, &[], &decompressed)
            .as_bytes()
            .to_hex();
        if actual_hash != *hash {
            report.increment("block.misplaced", 1);
            error!(
                "Block file {:?} has actual decompressed hash {:?}",
                path, actual_hash
            );
            return Err(ErrorKind::BlockCorrupt(hash.clone()).into());
        }
        Ok(decompressed)
    }
}

fn hash_bytes(in_buf: &[u8]) -> Result<BlockHash> {
    let mut hasher = Blake2b::new(BLAKE_HASH_SIZE_BYTES);
    hasher.update(in_buf);
    Ok(hasher.finalize().as_bytes().to_hex())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use std::io::SeekFrom;
    use tempdir;
    use tempfile;

    use super::super::*;

    const EXAMPLE_TEXT: &'static [u8] = b"hello!";
    const EXAMPLE_BLOCK_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd\
         3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    fn make_example_file() -> tempfile::NamedTempFile {
        let mut tf = tempfile::NamedTempFile::new().unwrap();
        tf.write_all(EXAMPLE_TEXT).unwrap();
        tf.flush().unwrap();
        tf.seek(SeekFrom::Start(0)).unwrap();
        tf
    }

    fn setup() -> (tempdir::TempDir, BlockDir) {
        let testdir = tempdir::TempDir::new("block_test").unwrap();
        let block_dir = BlockDir::new(testdir.path());
        (testdir, block_dir)
    }

    #[test]
    pub fn write_to_file() {
        let expected_hash = EXAMPLE_BLOCK_HASH.to_string();
        let report = Report::new();
        let (testdir, mut block_dir) = setup();
        let mut example_file = make_example_file();

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), false);

        let (refs, hash_hex) = block_dir.store(&mut example_file, &report).unwrap();
        assert_eq!(hash_hex, EXAMPLE_BLOCK_HASH);

        // Should be in one block, and as it's currently unsalted the hash is the same.
        assert_eq!(1, refs.len());
        assert_eq!(0, refs[0].start);
        assert_eq!(EXAMPLE_BLOCK_HASH, refs[0].hash);

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
        let back = block_dir.get(&refs[0], &read_report).unwrap();
        assert_eq!(back, EXAMPLE_TEXT);
        assert_eq!(read_report.get_count("block.read"), 1);
        assert_eq!(
            read_report.get_size("block"),
            Sizes {
                uncompressed: EXAMPLE_TEXT.len() as u64,
                compressed: 8u64,
            }
        );
    }

    #[test]
    pub fn write_same_data_again() {
        let report = Report::new();
        let (_testdir, mut block_dir) = setup();

        let mut example_file = make_example_file();
        let (refs1, hash1) = block_dir.store(&mut example_file, &report).unwrap();
        assert_eq!(report.get_count("block.already_present"), 0);
        assert_eq!(report.get_count("block.write"), 1);

        let mut example_file = make_example_file();
        let (refs2, hash2) = block_dir.store(&mut example_file, &report).unwrap();
        assert_eq!(report.get_count("block.already_present"), 1);
        assert_eq!(report.get_count("block.write"), 1);

        assert_eq!(hash1, hash2);
        assert_eq!(refs1, refs2);
    }

    #[test]
    // Large enough that it should break across blocks.
    pub fn large_file() {
        use super::MAX_BLOCK_SIZE;
        let report = Report::new();
        let (_testdir, mut block_dir) = setup();
        let mut tf = tempfile::NamedTempFile::new().unwrap();
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

        let (addrs, _overall_hash) = block_dir.store(&mut tf, &report).unwrap();
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
