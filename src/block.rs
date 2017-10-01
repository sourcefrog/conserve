// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! File contents are stored in data blocks.
//!
//! The structure is: archive > band > blockdir > subdir > file.

use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
// use libflate::deflate;
use rustc_serialize::hex::ToHex;

use tempfile;

use super::compress::Compression;
use super::compress::snappy::Snappy;
use super::errors::*;
use super::report::{Report, Sizes};

/// Use the maximum 64-byte hash.
const BLAKE_HASH_SIZE_BYTES: usize = 64;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

/// The unique identifier for a block: its hexadecimal `BLAKE2b` hash.
pub type BlockHash = String;


/// Points to some compressed data inside the block dir.
///
/// Identifiers are: which file contains it, at what (pre-compression) offset,
/// and what (pre-compression) length.
#[derive(Debug, PartialEq, RustcDecodable, RustcEncodable)]
pub struct Address {
    /// ID of the block storing this info (in future, salted.)
    pub hash: String,

    /// Position in this block where data begins.
    pub start: u64,

    /// Length of this block to be used.
    pub len: u64,
}


/// A readable, writable directory within a band holding data blocks.
pub struct BlockDir {
    pub path: PathBuf,

    // Reusable buffer for reading input.
    in_buf: Vec<u8>,
}

fn block_name_to_subdirectory(block_hash: &str) -> &str {
    &block_hash[..SUBDIR_NAME_CHARS]
}

impl BlockDir {
    /// Create a BlockDir accessing `path`, which must exist as a directory.
    pub fn new(path: &Path) -> BlockDir {
        BlockDir {
            path: path.to_path_buf(),
            in_buf: Vec::<u8>::with_capacity(1 << 20),
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

    pub fn store(&mut self,
                 from_file: &mut Read,
                 report: &Report)
                 -> Result<(Vec<Address>, BlockHash)> {
        // TODO: Split large files, combine small files.
        self.in_buf.truncate(0);
        let uncomp_len = report.measure_duration("source.read",
            || from_file.read_to_end(&mut self.in_buf))? as u64;
        assert_eq!(self.in_buf.len() as u64, uncomp_len);

        let hex_hash = report.measure_duration("block.hash", || hash_bytes(&self.in_buf))?;

        let refs = vec![Address {
            hash: hex_hash.clone(),
            start: 0,
            len: uncomp_len as u64,
        }];

        if self.contains(&hex_hash)? {
            report.increment("block.already_present", 1);
            return Ok((refs, hex_hash));
        }

        // Not already stored: compress and save it now.
        let comp_len = self.compress_and_store(&self.in_buf, &hex_hash, &report)?;
        report.increment("block", 1);
        report.increment_size("block",
            Sizes {
                compressed: comp_len,
                uncompressed: uncomp_len,
            });
        Ok((refs, hex_hash))
    }

    fn compress_and_store(&self, in_buf: &[u8], hex_hash: &BlockHash, report: &Report) -> Result<u64> {
        super::io::ensure_dir_exists(&self.subdir_for(hex_hash))?;
        let tempf = try!(tempfile::NamedTempFileOptions::new()
            .prefix("tmp")
            .create_in(&self.path));
        let mut bufw = io::BufWriter::new(tempf);
        report.measure_duration("block.compress",
            || Snappy::compress_and_write(&in_buf, &mut bufw))?;
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
    /// TODO: Return a Read rather than a Vec.
    pub fn get(self: &BlockDir, addr: &Address, report: &Report) -> Result<Vec<u8>> {
        // TODO: Accept vectors of multiple addresess, maybe in another function.
        let hash = &addr.hash;
        assert_eq!(0, addr.start);
        let path = self.path_for_file(hash);
        let mut f = try!(fs::File::open(&path));

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
        assert_eq!(decompressed.len(), addr.len as usize);
        report.increment("block", 1);
        report.increment_size("block",
                              Sizes {
                                  uncompressed: decompressed.len() as u64,
                                  compressed: compressed_len as u64,
                              });

        let actual_hash = blake2b::blake2b(BLAKE_HASH_SIZE_BYTES, &[], &decompressed)
            .as_bytes()
            .to_hex();
        if actual_hash != *hash {
            report.increment("block.misplaced", 1);
            error!("Block file {:?} has actual decompressed hash {:?}",
                   path,
                   actual_hash);
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
    use std::io::SeekFrom;
    use std::io::prelude::*;
    use tempdir;
    use tempfile;

    use super::BlockDir;
    use report::{Report, Sizes};

    const EXAMPLE_TEXT: &'static [u8] = b"hello!";
    const EXAMPLE_BLOCK_HASH: &'static str = "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

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

        assert_eq!(report.borrow_counts().get_count("block.already_present"), 0);
        assert_eq!(report.borrow_counts().get_count("block"), 1);
        let sizes = report.borrow_counts().get_size("block");
        assert_eq!(sizes.uncompressed, 6);

        // Will vary depending on compressor and we don't want to be too brittle.
        assert!(sizes.compressed <= 19, sizes.compressed);

        // Try to read back
        let read_report = Report::new();
        assert_eq!(read_report.borrow_counts().get_count("block"), 0);
        let back = block_dir.get(&refs[0], &read_report).unwrap();
        assert_eq!(back, EXAMPLE_TEXT);
        {
            let counts = read_report.borrow_counts();
            assert_eq!(counts.get_count("block"), 1);
            assert_eq!(counts.get_size("block"),
                       Sizes {
                           uncompressed: EXAMPLE_TEXT.len() as u64,
                           compressed: 8u64,
                       });
        }
    }

    #[test]
    pub fn write_same_data_again() {
        let report = Report::new();
        let (_testdir, mut block_dir) = setup();

        let mut example_file = make_example_file();
        let (refs1, hash1) = block_dir.store(&mut example_file, &report).unwrap();
        assert_eq!(report.borrow_counts().get_count("block.already_present"), 0);
        assert_eq!(report.borrow_counts().get_count("block"), 1);

        let mut example_file = make_example_file();
        let (refs2, hash2) = block_dir.store(&mut example_file, &report).unwrap();
        assert_eq!(report.borrow_counts().get_count("block.already_present"), 1);
        assert_eq!(report.borrow_counts().get_count("block"), 1);

        assert_eq!(hash1, hash2);
        assert_eq!(refs1, refs2);
    }
}
