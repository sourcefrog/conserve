// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

use derive_more::{Add, AddAssign};

#[derive(Add, AddAssign, Debug, Default, Eq, PartialEq, Clone)]
pub struct CopyStats {
    pub files: usize,
    pub symlinks: usize,
    pub directories: usize,
    pub unknown_kind: usize,

    pub files_unmodified: usize,
    pub files_modified: usize,
    pub files_new: usize,

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
}

#[derive(Add, AddAssign, Clone, Debug, Default, Eq, PartialEq)]
pub struct IndexBuilderStats {
    pub hunk_count: u64,
}
