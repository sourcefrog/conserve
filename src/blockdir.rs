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
//! The structure is: archive > blockdir > subdir > file.

use std::fs;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use blake2_rfc::blake2b;
use blake2_rfc::blake2b::Blake2b;
use rustc_serialize::hex::ToHex;

use rayon::prelude::*;
use tempfile;

use super::*;

/// Use the maximum 64-byte hash.
pub const BLAKE_HASH_SIZE_BYTES: usize = 64;

const BLOCKDIR_FILE_NAME: usize = BLAKE_HASH_SIZE_BYTES * 2;

/// Take this many characters from the block hash to form the subdirectory name.
const SUBDIR_NAME_CHARS: usize = 3;

const TMP_PREFIX: &str = "tmp";

/// Break blocks at this many uncompressed bytes.
pub const MAX_BLOCK_SIZE: usize = 1 << 20;

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
        fs::create_dir(path)?;
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

    /// Store the contents of a readable file into the BlockDir.
    ///
    /// Returns the addresses at which it was stored, plus the hash of the overall original file.
    pub fn store(
        &mut self,
        from_file: &mut Read,
        report: &Report,
    ) -> Result<(Vec<Address>, BlockHash)> {
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
            let read_len = from_file.read(&mut in_buf)?;
            if read_len == 0 {
                break;
            }
            in_buf.truncate(read_len);

            let block_hash: String;
            if addresses.is_empty() {
                file_hasher.update(&in_buf);
                block_hash = file_hasher.clone().finalize().as_bytes().to_hex()
            } else {
                // Not the first block, must update file and block hash separately, but we can do
                // them in parallel.
                block_hash = rayon::join(
                    || file_hasher.update(&in_buf),
                    || hash_bytes(&in_buf).unwrap(),
                )
                .1;
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
        Ok((addresses, file_hasher.finalize().as_bytes().to_hex()))
    }

    fn compress_and_store(&self, in_buf: &[u8], hex_hash: &str, report: &Report) -> Result<u64> {
        // Note: When we come to support cloud storage, we should do one atomic write rather than
        // a write and rename.
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
        if let Err(e) = tempf.persist(&self.path_for_file(&hex_hash)) {
            if e.error.kind() == io::ErrorKind::AlreadyExists {
                // Suprising we saw this rather than detecting it above.
                report.problem(&format!(
                    "Unexpected late detection of existing block {:?}",
                    hex_hash
                ));
                report.increment("block.already_present", 1);
            } else {
                return Err(e.error.into());
            }
        }
        Ok(comp_len)
    }

    /// True if the named block is present in this directory.
    pub fn contains(&self, hash: &str) -> Result<bool> {
        match fs::metadata(self.path_for_file(hash)) {
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Ok(_) => Ok(true),
            Err(e) => Err(e.into()),
        }
    }

    /// Get an object accessing a whole block.
    /// The contents are not yet narrowed down to only the addressed region.
    pub fn get_block(&self, hash: &str) -> Block {
        Block {
            path: self.path_for_file(&hash),
            hash: hash.to_string(),
        }
    }

    /// Read back the contents of a block, as a byte array.
    ///
    /// To read a whole file, use StoredFile instead.
    pub fn get(&self, addr: &Address, report: &Report) -> Result<Vec<u8>> {
        // TODO: Return a Read rather than a Vec?
        if addr.start != 0 {
            unimplemented!();
        }
        let b = self.get_block(&addr.hash);
        let decompressed = b.get_all(report)?;
        // TODO: Accept addresses referring to only part of a block.
        if decompressed.len() != addr.len as usize {
            unimplemented!();
        }
        Ok(decompressed)
    }

    /// Return a sorted vec of prefix subdirectories.
    fn subdirs(&self, report: &Report) -> Result<Vec<String>> {
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

    /// Return a sorted vec of all the blocknames in the blockdir.
    pub fn block_names(&self, report: &Report) -> Result<Vec<String>> {
        // The vecs from `subdirs` and `list_dir` are already sorted, so
        // we don't need to sort here.
        Ok(self
            .subdirs(report)?
            .iter()
            .flat_map(|s| {
                let (mut fs, _ds) = list_dir(&self.path.join(s)).unwrap();
                fs.retain(|ff| {
                    if ff.starts_with(TMP_PREFIX) {
                        false
                    } else if ff.len() != BLOCKDIR_FILE_NAME {
                        report.problem(&format!("unlikely file name in {:?}: {:?}", self, ff));
                        false
                    } else {
                        true
                    }
                });
                fs.into_iter()
            })
            .collect())
    }

    pub fn blocks(&self, report: &Report) -> Result<Vec<Block>> {
        Ok(self.block_names(report)?.iter().map(|b| self.get_block(b)).collect::<Vec<Block>>())
    }

    /// Check format invariants of the BlockDir; report any problems to the Report.
    pub fn validate(&self, report: &Report) -> Result<()> {
        // TODO: In the top-level directory, no files or directories other than prefix
        // directories of the right length.
        report.set_phase("Count blocks");
        let bs = self.blocks(report)?;
        let tot = bs.iter().try_fold(
            0u64,
            |t, b| Ok(t + b.compressed_size()?) as Result<u64> )?;
        report.set_total_work(tot);

        report.set_phase("Check block hashes");
        bs.par_iter()
            .map(|b| {
                report.increment_work(b.compressed_size()?);
                b.validate(report)
            })
            .try_for_each(|i| i)?;
        Ok(())
    }
}

/// Read-only access to one block in the BlockDir.
#[derive(Clone, Debug)]
pub struct Block {
    // TODO: Maybe hold an Rc on the root path and compute the block's path on demand?
    path: PathBuf,
    hash: String,
}

impl Block {
    /// Return the entire contents of the block.
    pub fn get_all(&self, report: &Report) -> Result<Vec<u8>> {
        let mut f = File::open(&self.path.as_path())?;
        // TODO: Specific error for compression failure (corruption?) vs io errors.
        let (compressed_len, de) = match Snappy::decompress_read(&mut f) {
            Ok(d) => d,
            Err(e) => {
                report.increment("block.corrupt", 1);
                report.problem(&format!("Block file {:?} read error {:?}", self.path, e));
                return Err(Error::BlockCorrupt(self.path.clone()));
            }
        };
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

    pub fn validate(&self, report: &Report) -> Result<()> {
        let de = self.get_all(report)?;

        let actual_hash = blake2b::blake2b(BLAKE_HASH_SIZE_BYTES, &[], &de)
            .as_bytes()
            .to_hex();
        if actual_hash != *self.hash {
            report.increment("block.misplaced", 1);
            report.problem(&format!(
                "Block file {:?} has actual decompressed hash {:?}",
                self.path, actual_hash
            ));
            return Err(Error::BlockCorrupt(self.path.clone()));
        }
        Ok(())
    }

    pub fn compressed_size(&self) -> Result<u64> {
        Ok(fs::metadata(&self.path)?.len())
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
    use tempfile::{NamedTempFile, TempDir};

    use super::super::*;

    const EXAMPLE_TEXT: &'static [u8] = b"hello!";
    const EXAMPLE_BLOCK_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd\
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

        // Block should be the one block present in the list.
        assert_eq!(
            block_dir.block_names(&Report::new()).unwrap(),
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

        // Validate
        let validate_r = Report::new();
        block_dir.validate(&validate_r).unwrap();
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
