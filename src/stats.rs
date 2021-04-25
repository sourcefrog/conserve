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

use std::fmt;
use std::io;
use std::time::Duration;

use derive_more::{Add, AddAssign};
use thousands::Separable;

use crate::ui::duration_to_hms;
use crate::*;

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

fn write_size<I: Into<u64>>(w: &mut fmt::Formatter<'_>, label: &str, value: I) {
    writeln!(w, "{:>12} MB   {}", mb_string(value.into()), label).unwrap();
}

fn write_compressed_size(w: &mut fmt::Formatter<'_>, compressed: u64, uncompressed: u64) {
    write_size(w, "uncompressed", uncompressed);
    write_size(
        w,
        &format!("after {:.1}x compression", ratio(uncompressed, compressed)),
        compressed,
    );
}

fn write_count<I: Into<usize>>(w: &mut fmt::Formatter<'_>, label: &str, value: I) {
    writeln!(
        w,
        "{:>12}      {}",
        value.into().separate_with_commas(),
        label
    )
    .unwrap();
}

fn write_duration(w: &mut fmt::Formatter<'_>, label: &str, duration: Duration) -> fmt::Result {
    writeln!(w, "{:>12}      {}", duration_to_hms(duration), label)
}

/// Describes sizes of data read or written, with both the
/// compressed and uncompressed size.
#[derive(Add, AddAssign, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sizes {
    pub compressed: u64,
    pub uncompressed: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Add, AddAssign)]
pub struct ValidateStats {
    /// Count of files in the wrong place.
    pub structure_problems: usize,
    pub io_errors: usize,

    /// Failed to open a band.
    pub band_open_errors: usize,

    /// Failed to open a stored tree.
    pub tree_open_errors: usize,
    pub tree_validate_errors: usize,

    pub band_metadata_problems: usize,

    /// Count of files not expected to be in the archive.
    pub unexpected_files: usize,
    pub missing_band_heads: usize,

    /// Number of blocks read.
    pub block_read_count: u64,
    /// Number of blocks that failed to read back.
    pub block_error_count: usize,
    pub block_missing_count: usize,
}

impl ValidateStats {
    pub fn summarize(&self, write: &mut dyn io::Write) -> Result<()> {
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
        writeln!(write, "{:#?}", self).map_err(Error::from)
    }

    pub fn has_problems(&self) -> bool {
        self.block_error_count > 0 || self.io_errors > 0 || self.block_missing_count > 0
    }
}

#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct IndexReadStats {
    pub index_hunks: usize,
    pub uncompressed_index_bytes: u64,
    pub compressed_index_bytes: u64,
    pub errors: usize,
}

#[derive(Add, AddAssign, Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexWriterStats {
    pub index_hunks: usize,
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
pub struct RestoreStats {
    pub files: usize,
    pub symlinks: usize,
    pub directories: usize,
    pub unknown_kind: usize,

    pub errors: usize,

    pub uncompressed_file_bytes: u64,

    // TODO: Include elapsed time.
}

impl fmt::Display for RestoreStats {
    fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_count(w, "files:", self.files);
        write_size(w, "  ", self.uncompressed_file_bytes);

        write_count(w, "symlinks", self.symlinks);
        write_count(w, "directories", self.directories);
        write_count(w, "unsupported file kind", self.unknown_kind);
        writeln!(w).unwrap();

        write_count(w, "errors", self.errors);

        Ok(())
    }
}

#[derive(Add, AddAssign, Debug, Default, Eq, PartialEq, Clone)]
pub struct BackupStats {
    // TODO: Have separate more-specific stats for backup and restore, and then
    // each can have a single Display method.
    // TODO: Include source file bytes, including unmodified files.
    pub files: usize,
    pub symlinks: usize,
    pub directories: usize,
    pub unknown_kind: usize,

    pub unmodified_files: usize,
    pub modified_files: usize,
    pub new_files: usize,

    /// Bytes that matched an existing block.
    pub deduplicated_bytes: u64,
    /// Bytes that were stored as new blocks, before compression.
    pub uncompressed_bytes: u64,
    pub compressed_bytes: u64,

    pub deduplicated_blocks: usize,
    pub written_blocks: usize,
    /// Blocks containing combined small files.
    pub combined_blocks: usize,

    pub empty_files: usize,
    pub small_combined_files: usize,
    pub single_block_files: usize,
    pub multi_block_files: usize,

    pub errors: usize,

    pub index_builder_stats: IndexWriterStats,
    // TODO: Include elapsed time.
}

impl fmt::Display for BackupStats {
    fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_count(w, "files:", self.files);
        write_count(w, "  unmodified files", self.unmodified_files);
        write_count(w, "  modified files", self.modified_files);
        write_count(w, "  new files", self.new_files);
        write_count(w, "symlinks", self.symlinks);
        write_count(w, "directories", self.directories);
        write_count(w, "unsupported file kind", self.unknown_kind);
        writeln!(w).unwrap();

        write_count(w, "files stored:", self.new_files + self.modified_files);
        write_count(w, "  empty files", self.empty_files);
        write_count(w, "  small combined files", self.small_combined_files);
        write_count(w, "  single block files", self.single_block_files);
        write_count(w, "  multi-block files", self.multi_block_files);
        writeln!(w).unwrap();

        write_count(w, "data blocks deduplicated:", self.deduplicated_blocks);
        write_size(w, "  saved", self.deduplicated_bytes);
        writeln!(w).unwrap();

        write_count(w, "new data blocks written:", self.written_blocks);
        write_count(w, "  blocks of combined files", self.combined_blocks);
        write_compressed_size(w, self.compressed_bytes, self.uncompressed_bytes);
        writeln!(w).unwrap();

        let idx = &self.index_builder_stats;
        write_count(w, "new index hunks", idx.index_hunks);
        write_compressed_size(w, idx.compressed_index_bytes, idx.uncompressed_index_bytes);
        writeln!(w).unwrap();

        write_count(w, "errors", self.errors);

        Ok(())
    }
}

#[derive(Add, AddAssign, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeleteStats {
    pub deleted_band_count: usize,
    pub unreferenced_block_count: usize,
    pub unreferenced_block_bytes: u64,
    pub deletion_errors: usize,
    pub deleted_block_count: usize,
    pub elapsed: Duration,
}

impl fmt::Display for DeleteStats {
    fn fmt(&self, w: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(w, "deletion stats",)?;

        write_count(w, "bands deleted", self.deleted_band_count);
        writeln!(w)?;

        write_count(w, "unreferenced blocks", self.unreferenced_block_count);
        write_size(w, "  unreferenced", self.unreferenced_block_bytes);
        write_count(w, "  deleted", self.deleted_block_count);
        writeln!(w)?;

        write_count(w, "deletion errors", self.deletion_errors);
        writeln!(w)?;

        write_duration(w, "elapsed", self.elapsed)?;

        Ok(())
    }
}
