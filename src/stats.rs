// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

use std::io;

use derive_more::{Add, AddAssign};
use thousands::Separable;

use crate::Result;

pub fn mb_string(s: u64) -> String {
    (s / 1_000_000).separate_with_commas()
}

/// Describe the compression ratio: higher is better.
fn ratio(uncompressed: u64, compressed: u64) -> f64 {
    if compressed > 0 {
        uncompressed as f64 / compressed as f64
    } else {
        0f64
    }
}

/// Describes sizes of data read or written, with both the
/// compressed and uncompressed size.
#[derive(Add, AddAssign, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sizes {
    pub compressed: u64,
    pub uncompressed: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ValidateArchiveStats {
    pub block_dir_stats: ValidateBlockDirStats,
}

impl ValidateArchiveStats {
    pub fn summarize(&self, _to_write: &mut dyn io::Write) -> Result<()> {
        // format!(
        //     "{:>12} MB   in {} blocks.\n\
        //      {:>12} MB/s block validation rate.\n\
        //      {:>12}      elapsed.\n",
        //     (self.get_size("block").uncompressed / M).separate_with_commas(),
        //     self.get_count("block.read").separate_with_commas(),
        //     (mbps_rate(self.get_size("block").uncompressed, self.elapsed_time()) as u64)
        //         .separate_with_commas(),
        //     duration_to_hms(self.elapsed_time()),
        // )
        Ok(())
    }
}

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct ValidateBlockDirStats {
    /// Number of blocks read.
    pub block_read_count: u64,
    /// Number of blocks that failed to read back.
    pub block_error_count: u64,
}

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct IndexEntryIterStats {
    pub index_hunks: u64,
    pub uncompressed_index_bytes: u64,
    pub compressed_index_bytes: u64,
}

#[derive(Add, AddAssign, Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexBuilderStats {
    pub index_hunks: u64,
    pub uncompressed_index_bytes: u64,
    pub compressed_index_bytes: u64,
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct LiveTreeIterStats {
    pub directories_visited: usize,
    pub exclusions: usize,
    pub metadata_error: usize,
    pub entries_returned: usize,
}

#[derive(Add, AddAssign, Debug, Default, Eq, PartialEq, Clone)]
pub struct CopyStats {
    // TODO: Have separate more-specific stats for backup and restore, and then
    // each can have a single Display method.
    pub files: usize,
    pub symlinks: usize,
    pub directories: usize,
    pub unknown_kind: usize,

    pub unmodified_files: usize,
    pub modified_files: usize,
    pub new_files: usize,

    // TODO: Include source file bytes, including unmodified files.
    pub deduplicated_bytes: u64,
    pub uncompressed_bytes: u64,
    pub compressed_bytes: u64,

    pub deduplicated_blocks: usize,
    pub written_blocks: usize,

    pub empty_files: usize,
    pub single_block_files: usize,
    pub multi_block_files: usize,

    pub errors: usize,

    pub index_builder_stats: IndexBuilderStats,
    // TODO: Include elapsed time.
}

impl CopyStats {
    pub fn summarize_restore(&self, _to_stream: &mut dyn io::Write) -> Result<()> {
        // format!(
        //     "{:>12} MB   in {} files, {} directories, {} symlinks.\n\
        //      {:>12} MB/s output rate.\n\
        //      {:>12} MB   after deduplication.\n\
        //      {:>12} MB   in {} blocks after {:.1}x compression.\n\
        //      {:>12} MB   in {} compressed index hunks.\n\
        //      {:>12}      elapsed.\n",
        //     (self.get_size("file.bytes").uncompressed / M).separate_with_commas(),
        //     self.get_count("file").separate_with_commas(),
        //     self.get_count("dir").separate_with_commas(),
        //     self.get_count("symlink").separate_with_commas(),
        //     (mbps_rate(
        //         self.get_size("file.bytes").uncompressed,
        //         self.elapsed_time()
        //     ) as u64)
        //         .separate_with_commas(),
        //     (self.get_size("block").uncompressed / M).separate_with_commas(),
        //     (self.get_size("block").compressed / M).separate_with_commas(),
        //     self.get_count("block.read").separate_with_commas(),
        //     compression_ratio(&self.get_size("block")),
        //     (self.get_size("index").compressed / M).separate_with_commas(),
        //     self.get_count("index.hunk").separate_with_commas(),
        //     duration_to_hms(self.elapsed_time()),
        Ok(())
    }

    pub fn summarize_backup(&self, w: &mut dyn io::Write) {
        // TODO: Perhaps summarize to a string, or make this the Display impl.
        writeln!(w, "{:>12}      files:", self.files.separate_with_commas()).unwrap();
        writeln!(
            w,
            "{:>12}        unmodified files",
            self.unmodified_files.separate_with_commas()
        )
        .unwrap();
        writeln!(
            w,
            "{:>12}        modified files",
            self.modified_files.separate_with_commas()
        )
        .unwrap();
        writeln!(
            w,
            "{:>12}        new files",
            self.new_files.separate_with_commas()
        )
        .unwrap();
        writeln!(
            w,
            "{:>12}      symlinks",
            self.symlinks.separate_with_commas()
        )
        .unwrap();
        writeln!(
            w,
            "{:>12}      directories",
            self.directories.separate_with_commas()
        )
        .unwrap();
        writeln!(
            w,
            "{:>12}      special files skipped",
            self.unknown_kind.separate_with_commas(),
        )
        .unwrap();
        writeln!(w).unwrap();

        writeln!(
            w,
            "{:>12}      deduplicated data blocks:",
            self.deduplicated_blocks.separate_with_commas(),
        )
        .unwrap();
        writeln!(w, "{:>12} MB     saved", mb_string(self.deduplicated_bytes),).unwrap();
        writeln!(
            w,
            "{:>12}      new data blocks:",
            self.written_blocks.separate_with_commas(),
        )
        .unwrap();
        writeln!(
            w,
            "{:>12} MB     uncompressed",
            mb_string(self.uncompressed_bytes),
        )
        .unwrap();
        writeln!(
            w,
            "{:>12} MB     after {:.1}x compression",
            mb_string(self.compressed_bytes),
            ratio(self.uncompressed_bytes, self.compressed_bytes)
        )
        .unwrap();

        writeln!(w).unwrap();
        let idx = &self.index_builder_stats;
        writeln!(
            w,
            "{:>12}      new index hunks:",
            idx.index_hunks.separate_with_commas(),
        )
        .unwrap();
        writeln!(
            w,
            "{:>12} MB     uncompressed",
            mb_string(idx.uncompressed_index_bytes),
        )
        .unwrap();
        writeln!(
            w,
            "{:>12} MB     after {:.1}x compression",
            mb_string(idx.compressed_index_bytes),
            ratio(idx.uncompressed_index_bytes, idx.compressed_index_bytes),
        )
        .unwrap();
        writeln!(w).unwrap();
        writeln!(w, "{:>12}      errors", self.errors.separate_with_commas()).unwrap();

        // format!(
        //     "{:>12} MB   in {} files, {} directories, {} symlinks.\n\
        //      {:>12}      files are unchanged.\n\
        //      {:>12} MB/s input rate.\n\
        //      {:>12} MB   after deduplication.\n\
        //      {:>12} MB   in {} blocks after {:.1}x compression.\n\
        //      {:>12} MB   in {} index hunks after {:.1}x compression.\n\
        //      {:>12}      elapsed.\n",
        //     (self.get_size("file.bytes").uncompressed / M).separate_with_commas(),
        //     self.get_count("file").separate_with_commas(),
        //     self.get_count("dir").separate_with_commas(),
        //     self.get_count("symlink").separate_with_commas(),
        //     self.get_count("file.unchanged").separate_with_commas(),
        //     (mbps_rate(
        //         self.get_size("file.bytes").uncompressed,
        //         self.elapsed_time()
        //     ) as u64)
        //         .separate_with_commas(),
        //     (self.get_size("block").uncompressed / M).separate_with_commas(),
        //     (self.get_size("block").compressed / M).separate_with_commas(),
        //     self.get_count("block.write").separate_with_commas(),
        //     compression_ratio(&self.get_size("block")),
        //     (self.get_size("index").compressed / M).separate_with_commas(),
        //     self.get_count("index.hunk").separate_with_commas(),
        //     compression_ratio(&self.get_size("index")),
        //     duration_to_hms(self.elapsed_time()),
        // )
    }
}
