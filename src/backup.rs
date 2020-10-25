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

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::convert::TryInto;
use std::io::prelude::*;

use globset::GlobSet;
use itertools::Itertools;

use crate::blockdir::Address;
use crate::index::IndexEntryIter;
use crate::stats::CopyStats;
use crate::tree::ReadTree;
use crate::*;

/// Configuration of how to make a backup.
#[derive(Debug, Default)]
pub struct BackupOptions {
    /// Print filenames to the UI as they're copied.
    pub print_filenames: bool,

    /// Exclude these globs from the backup.
    pub excludes: Option<GlobSet>,
}

// This causes us to walk the source tree twice, which is probably an acceptable option
// since it's nice to see realistic overall progress. We could keep all the entries
// in memory, and maybe we should, but it might get unreasonably big.
// if options.measure_first {
//     progress_bar.set_phase("Measure source tree".to_owned());
//     // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
//     // again a second time? But, that'll potentially use memory proportional to tree size, which
//     // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
//     // deleted or changed while this is running.
//     progress_bar.set_bytes_total(source.size()?.file_bytes as u64);
// }

/// Backup a source directory into a new band in the archive.
///
/// Returns statistics about what was copied.
pub fn backup(archive: &Archive, source: &LiveTree, options: &BackupOptions) -> Result<CopyStats> {
    let mut writer = BackupWriter::begin(archive)?;
    let mut stats = CopyStats::default();
    let mut progress_bar = ProgressBar::new();

    progress_bar.set_phase("Copying".to_owned());
    let entry_iter = source.iter_filtered(None, options.excludes.clone())?;
    for entry_group in entry_iter
        .chunks(crate::index::MAX_ENTRIES_PER_HUNK)
        .into_iter()
    {
        for entry in entry_group {
            if options.print_filenames {
                crate::ui::println(entry.apath());
            }
            progress_bar.set_filename(entry.apath().to_string());
            if let Err(e) = writer.copy_entry(&entry, source) {
                ui::show_error(&e);
                stats.errors += 1;
                continue;
            }
            progress_bar.increment_bytes_done(entry.size().unwrap_or(0));
        }
        writer.flush_group()?;
    }
    stats += writer.finish()?;
    // TODO: Merge in stats from the tree iter and maybe the source tree?
    Ok(stats)
}

/// Accepts files to write in the archive (in apath order.)
struct BackupWriter {
    band: Band,
    index_builder: IndexBuilder,
    stats: CopyStats,
    block_dir: BlockDir,
    buffer: Buffer,

    /// The index for the last stored band, used as hints for whether newly
    /// stored files have changed.
    basis_index: Option<IndexEntryIter>,
}

impl BackupWriter {
    /// Create a new BackupWriter.
    ///
    /// This currently makes a new top-level band.
    pub fn begin(archive: &Archive) -> Result<BackupWriter> {
        if gc_lock::GarbageCollectionLock::is_locked(archive)? {
            return Err(Error::GarbageCollectionLockHeld);
        }
        // TODO: Use a stitched band as the basis.
        // <https://github.com/sourcefrog/conserve/issues/142>
        let basis_index = archive
            .last_complete_band()?
            .map(|b| b.iter_entries())
            .transpose()?;
        // Create the new band only after finding the basis band!
        let band = Band::create(archive)?;
        let index_builder = band.index_builder();
        Ok(BackupWriter {
            band,
            index_builder,
            buffer: Buffer::new(),
            block_dir: archive.block_dir().clone(),
            stats: CopyStats::default(),
            basis_index,
        })
    }

    fn finish(self) -> Result<CopyStats> {
        let index_builder_stats = self.index_builder.finish()?;
        self.band.close(index_builder_stats.index_hunks)?;
        Ok(CopyStats {
            index_builder_stats,
            ..self.stats
        })
    }

    /// Write out any pending data blocks, and then the pending index entries.
    fn flush_group(&mut self) -> Result<()> {
        // TODO: Finish any compression groups.
        self.index_builder.finish_hunk()
    }

    fn copy_entry<R: ReadTree>(&mut self, entry: &R::Entry, source: &R) -> Result<()> {
        match entry.kind() {
            Kind::Dir => self.copy_dir(entry),
            Kind::File => self.copy_file(entry, source),
            Kind::Symlink => self.copy_symlink(entry),
            Kind::Unknown => {
                self.stats.unknown_kind += 1;
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                Ok(())
            }
        }
    }

    fn copy_dir<E: Entry>(&mut self, source_entry: &E) -> Result<()> {
        // TODO: Pass back index sizes
        self.stats.directories += 1;
        self.index_builder
            .push_entry(IndexEntry::metadata_from(source_entry));
        Ok(())
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file<R: ReadTree>(&mut self, source_entry: &R::Entry, from_tree: &R) -> Result<()> {
        self.stats.files += 1;
        let apath = source_entry.apath();
        if let Some(basis_entry) = self
            .basis_index
            .as_mut()
            .map(|bi| bi.advance_to(&apath))
            .flatten()
        {
            if source_entry.is_unchanged_from(&basis_entry) {
                // TODO: In verbose mode, say if the file is changed, unchanged,
                // etc, but without duplicating the filenames.
                //
                // ui::println(&format!("unchanged file {}", apath));

                // We can reasonably assume that the existing archive complies
                // with the archive invariants, which include that all the
                // blocks referenced by the index, are actually present.
                self.stats.unmodified_files += 1;
                self.index_builder.push_entry(basis_entry);
                return Ok(());
            } else {
                self.stats.modified_files += 1;
            }
        } else {
            self.stats.new_files += 1;
        }
        let read_source = from_tree.file_contents(&source_entry);
        let addrs = store_file_content(
            &apath,
            &mut read_source?,
            &mut self.block_dir,
            &mut self.buffer,
            &mut self.stats,
        )?;
        self.index_builder.push_entry(IndexEntry {
            addrs,
            ..IndexEntry::metadata_from(source_entry)
        });
        Ok(())
    }

    fn copy_symlink<E: Entry>(&mut self, source_entry: &E) -> Result<()> {
        let target = source_entry.symlink_target().clone();
        self.stats.symlinks += 1;
        assert!(target.is_some());
        self.index_builder
            .push_entry(IndexEntry::metadata_from(source_entry));
        Ok(())
    }
}

/// A reusable block-sized buffer.
struct Buffer(Vec<u8>);

impl Buffer {
    fn new() -> Buffer {
        Buffer(vec![0; MAX_BLOCK_SIZE])
    }
}

fn store_file_content(
    apath: &Apath,
    from_file: &mut dyn Read,
    block_dir: &mut BlockDir,
    buffer: &mut Buffer,
    stats: &mut CopyStats,
) -> Result<Vec<Address>> {
    let mut addresses = Vec::<Address>::with_capacity(1);
    loop {
        // TODO: Possibly read repeatedly in case we get a short read and have room for more,
        // so that short reads don't lead to short blocks being stored.
        let read_len = from_file
            .read(&mut buffer.0)
            .map_err(|source| Error::StoreFile {
                apath: apath.to_owned(),
                source,
            })?;
        if read_len == 0 {
            break;
        }
        let block_data = &buffer.0[..read_len];
        let hash = block_dir.store_or_deduplicate(block_data, stats)?;
        addresses.push(Address {
            hash,
            start: 0,
            len: read_len as u64,
        });
    }
    match addresses.len() {
        0 => stats.empty_files += 1,
        1 => stats.single_block_files += 1,
        _ => stats.multi_block_files += 1,
    }
    Ok(addresses)
}

/// Combines multiple small files into a single block.
///
/// When the block is finished, and only then, this returns the index entries with the addresses
/// completed.
struct FileCombiner {
    /// Buffer of concatenated data from small files.
    buf: Vec<u8>,
    queue: Vec<QueuedFile>,
    /// Entries for files that have been written to the blockdir, and that have complete addresses.
    finished: Vec<IndexEntry>,
    stats: CopyStats,
    block_dir: BlockDir,
}

/// A file in the process of being written into a combined block.
///
/// While this exists, the data has been stored into the combine buffer, and we know
/// the offset and length. But since the combine buffer hasn't been finished and hashed,
/// we do not yet know a full address.
struct QueuedFile {
    /// Offset of the start of the data from this file within `buf`.
    start: usize,
    /// Length of data in this file.
    len: usize,
    /// IndexEntry without addresses.
    entry: IndexEntry,
}

impl FileCombiner {
    fn new(block_dir: BlockDir) -> FileCombiner {
        FileCombiner {
            block_dir,
            buf: Vec::new(),
            queue: Vec::new(),
            finished: Vec::new(),
            stats: CopyStats::default(),
        }
    }

    /// Flush any pending files, and return accumulated file entries and stats.
    fn drain(mut self) -> Result<(CopyStats, Vec<IndexEntry>)> {
        self.flush()?;
        Ok((self.stats, self.finished))
    }

    /// Write all the content from the combined block to a blockdir.
    ///
    /// Returns the fully populated entries for all files in this combined block.
    ///
    /// After this call the FileCombiner is empty and can be reused for more files into a new
    /// block.
    fn flush(&mut self) -> Result<()> {
        if self.queue.is_empty() {
            assert!(self.buf.is_empty());
            return Ok(());
        }
        let hash = self
            .block_dir
            .store_or_deduplicate(&self.buf, &mut self.stats)?;
        self.buf.clear();
        self.finished
            .extend(self.queue.drain(..).map(|qf| IndexEntry {
                addrs: vec![Address {
                    hash: hash.clone(),
                    start: qf.start.try_into().unwrap(),
                    len: qf.len.try_into().unwrap(),
                }],
                ..qf.entry
            }));
        Ok(())
    }

    /// Add the contents of a small file into this combiner.
    ///
    /// `entry` should be an IndexEntry that's complete apart from the block addresses.
    fn push_file(&mut self, live_entry: &LiveEntry, from_file: &mut dyn Read) -> Result<()> {
        let start = self.buf.len();
        let expected_len: usize = live_entry
            .size()
            .expect("small file has no length")
            .try_into()
            .unwrap();
        let index_entry = IndexEntry::metadata_from(live_entry);
        if expected_len == 0 {
            self.finished.push(index_entry);
            return Ok(());
        }
        self.buf.resize(start + expected_len, 0);
        let len = from_file
            .read(&mut self.buf[start..])
            .map_err(|source| Error::StoreFile {
                apath: live_entry.apath().to_owned(),
                source,
            })?;
        self.buf.truncate(start + len);
        if len == 0 {
            self.stats.empty_files += 1;
            self.finished.push(index_entry);
        } else {
            self.stats.small_combined_files += 1;
            self.queue.push(QueuedFile {
                start,
                len,
                entry: index_entry,
            });
        }
        if self.buf.len() >= TARGET_COMBINED_BLOCK_SIZE {
            self.flush()
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::prelude::*;
    use std::io::{Cursor, SeekFrom};

    use tempfile::{NamedTempFile, TempDir};

    use crate::stats::Sizes;
    use crate::unix_time::UnixTime;

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
    fn store_a_file() {
        let expected_hash = EXAMPLE_BLOCK_HASH.to_string().parse().unwrap();
        let (testdir, mut block_dir) = setup();
        let mut example_file = make_example_file();

        assert_eq!(block_dir.contains(&expected_hash).unwrap(), false);

        let mut stats = CopyStats::default();
        let addrs = store_file_content(
            &Apath::from("/hello"),
            &mut example_file,
            &mut block_dir,
            &mut Buffer::new(),
            &mut stats,
        )
        .unwrap();

        // Should be in one block, with the expected hash.
        assert_eq!(1, addrs.len());
        assert_eq!(0, addrs[0].start);
        assert_eq!(addrs[0].hash, expected_hash);

        // Block should be the one block present in the list.
        let present_blocks = block_dir.block_names().unwrap().collect::<Vec<_>>();
        assert_eq!(present_blocks.len(), 1);
        assert_eq!(present_blocks[0], expected_hash);

        // Subdirectory and file should exist
        let expected_file = testdir.path().join("66a").join(EXAMPLE_BLOCK_HASH);
        let attr = fs::metadata(expected_file).unwrap();
        assert!(attr.is_file());

        // Compressed size is as expected.
        assert_eq!(block_dir.compressed_size(&expected_hash).unwrap(), 8);

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
        let (_testdir, mut block_dir) = setup();
        let addrs = store_file_content(
            &"/hello".into(),
            &mut Cursor::new(b"0123456789abcdef"),
            &mut block_dir,
            &mut Buffer::new(),
            &mut CopyStats::default(),
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
        let (_testdir, mut block_dir) = setup();
        let addrs = store_file_content(
            &"/hello".into(),
            &mut Cursor::new(b"0123456789abcdef"),
            &mut block_dir,
            &mut Buffer::new(),
            &mut CopyStats::default(),
        )
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
    fn write_same_data_again() {
        let (_testdir, mut block_dir) = setup();

        let mut example_file = make_example_file();
        let mut buffer = Buffer::new();
        let mut stats = CopyStats::default();
        let addrs1 = store_file_content(
            &Apath::from("/ello"),
            &mut example_file,
            &mut block_dir,
            &mut buffer,
            &mut stats,
        )
        .unwrap();
        assert_eq!(stats.deduplicated_blocks, 0);
        assert_eq!(stats.written_blocks, 1);
        assert_eq!(stats.uncompressed_bytes, 6);
        assert_eq!(stats.compressed_bytes, 8);

        let mut example_file = make_example_file();
        let mut stats2 = CopyStats::default();
        let addrs2 = store_file_content(
            &Apath::from("/ello2"),
            &mut example_file,
            &mut block_dir,
            &mut buffer,
            &mut stats2,
        )
        .unwrap();
        assert_eq!(stats2.deduplicated_blocks, 1);
        assert_eq!(stats2.written_blocks, 0);
        assert_eq!(stats2.compressed_bytes, 0);

        assert_eq!(addrs1, addrs2);
    }

    #[test]
    // Large enough that it should break across blocks.
    fn large_file() {
        use super::MAX_BLOCK_SIZE;
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

        let mut stats = CopyStats::default();
        let addrs = store_file_content(
            &Apath::from("/big"),
            &mut tf,
            &mut block_dir,
            &mut Buffer::new(),
            &mut stats,
        )
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

    #[test]
    fn combine_one_file() {
        let testdir = TempDir::new().unwrap();
        let block_dir = BlockDir::create_path(testdir.path()).unwrap();
        let mut combiner = FileCombiner::new(block_dir);
        let file_bytes = b"some stuff";
        let entry = LiveEntry {
            apath: Apath::from("/0"),
            kind: Kind::File,
            mtime: UnixTime {
                secs: 1603116230,
                nanosecs: 0,
            },
            symlink_target: None,
            size: Some(file_bytes.len() as u64),
        };
        let mut content = Cursor::new(file_bytes);
        combiner.push_file(&entry, &mut content).unwrap();
        let (stats, entries) = combiner.drain().unwrap();
        assert_eq!(entries.len(), 1);
        let addrs = entries[0].addrs.clone();
        assert_eq!(addrs.len(), 1, "combined file should have one block");
        assert_eq!(addrs[0].start, 0);
        assert_eq!(addrs[0].len, 10);
        let expected_entry = IndexEntry {
            addrs,
            ..IndexEntry::metadata_from(&entry)
        };
        assert_eq!(entries[0], expected_entry);
        assert_eq!(stats.uncompressed_bytes, 10);
        assert_eq!(stats.written_blocks, 1);
    }

    #[test]
    fn combine_several_small_files() {
        let testdir = TempDir::new().unwrap();
        let block_dir = BlockDir::create_path(testdir.path()).unwrap();
        let mut combiner = FileCombiner::new(block_dir);
        let file_bytes = b"some stuff";
        let mut live_entry = LiveEntry {
            apath: Apath::from("/0"),
            kind: Kind::File,
            mtime: UnixTime {
                secs: 1603116230,
                nanosecs: 0,
            },
            symlink_target: None,
            size: Some(file_bytes.len() as u64),
        };
        for i in 0..10 {
            live_entry.apath = Apath::from(format!("/{:02}", i));
            let mut content = Cursor::new(file_bytes);
            combiner.push_file(&live_entry, &mut content).unwrap();
        }
        let (stats, entries) = combiner.drain().unwrap();
        assert_eq!(entries.len(), 10);

        let first_hash = &entries[0].addrs[0].hash;

        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(
                entry.addrs,
                &[Address {
                    hash: first_hash.clone(),
                    start: i as u64 * 10,
                    len: file_bytes.len() as u64
                }]
            );
        }
        assert_eq!(stats.small_combined_files, 10);
        assert_eq!(stats.empty_files, 0);
        assert_eq!(stats.single_block_files, 0);
        assert_eq!(stats.multi_block_files, 0);
        assert_eq!(stats.uncompressed_bytes, 100);
        assert_eq!(stats.written_blocks, 1);
    }
}
