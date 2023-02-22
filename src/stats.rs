// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

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
use std::time::Duration;

use derive_more::{Add, AddAssign};
use thousands::Separable;

use crate::misc::duration_to_hms;

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

pub(crate) fn write_size<I: Into<u64>>(w: &mut fmt::Formatter<'_>, label: &str, value: I) {
    writeln!(w, "{:>12} MB   {}", mb_string(value.into()), label).unwrap();
}

pub(crate) fn write_compressed_size(
    w: &mut fmt::Formatter<'_>,
    compressed: u64,
    uncompressed: u64,
) {
    write_size(w, "uncompressed", uncompressed);
    write_size(
        w,
        &format!("after {:.1}x compression", ratio(uncompressed, compressed)),
        compressed,
    );
}

pub(crate) fn write_count<I: Into<usize>>(w: &mut fmt::Formatter<'_>, label: &str, value: I) {
    writeln!(
        w,
        "{:>12}      {}",
        value.into().separate_with_commas(),
        label
    )
    .unwrap();
}

pub(crate) fn write_duration(
    w: &mut fmt::Formatter<'_>,
    label: &str,
    duration: Duration,
) -> fmt::Result {
    writeln!(w, "{:>12}      {}", duration_to_hms(duration), label)
}

/// Describes sizes of data read or written, with both the
/// compressed and uncompressed size.
#[derive(Add, AddAssign, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Sizes {
    pub compressed: u64,
    pub uncompressed: u64,
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

    pub elapsed: Duration,
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
        write_duration(w, "elapsed", self.elapsed)?;

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
