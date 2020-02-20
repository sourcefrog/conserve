// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Conserve error types.

use std::path::PathBuf;

use snafu::Snafu;

use crate::*;

type IOError = std::io::Error;

/// Conserve specific error.
#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub enum Error {
    // TODO: Add messages and perhaps more fields to all of these.
    #[snafu(display("Block file {:?} corrupt; actual hash {:?}", path, actual_hash))]
    BlockCorrupt { path: PathBuf, actual_hash: String },

    #[snafu(display("Failed to store block {}", block_hash))]
    StoreBlock { block_hash: String, source: IOError },

    #[snafu(display("Failed to read block {:?}", path))]
    ReadBlock { path: PathBuf, source: IOError },

    #[snafu(display("Failed to list block files in {:?}", path))]
    ListBlocks { path: PathBuf, source: IOError },

    #[snafu(display("Not a Conserve archive: {:?}", path))]
    NotAnArchive { path: PathBuf },

    #[snafu(display("Failed to read archive header from {:?}", path))]
    ReadArchiveHeader {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display(
        "Archive version {:?} in {:?} is not supported by Conserve {}",
        version,
        path,
        crate::version()
    ))]
    UnsupportedArchiveVersion { path: PathBuf, version: String },

    #[snafu(display(
        "Band version {:?} in {:?} is not supported by Conserve {}",
        version,
        path,
        crate::version()
    ))]
    UnsupportedBandVersion { path: PathBuf, version: String },

    #[snafu(display("Destination directory not empty: {:?}", path))]
    DestinationNotEmpty { path: PathBuf },

    #[snafu(display("Archive has no bands"))]
    ArchiveEmpty,

    #[snafu(display("Invalid backup version number {:?}", version))]
    InvalidVersion { version: String },

    #[snafu(display("Failed to create band"))]
    CreateBand { source: std::io::Error },

    #[snafu(display("Failed to create block directory",))]
    CreateBlockDir { source: std::io::Error },

    #[snafu(display("Failed to create archive directory {:?}", path))]
    CreateArchiveDirectory {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Band {} is incomplete", band_id))]
    BandIncomplete { band_id: BandId },

    #[snafu(display("Failed to parse glob {:?}", glob))]
    ParseGlob {
        glob: String,
        source: globset::Error,
    },

    #[snafu(display("Failed to write index hunk {:?}", path))]
    WriteIndex { path: PathBuf, source: IOError },

    #[snafu(display("Failed to read index hunk {:?}", path))]
    ReadIndex { path: PathBuf, source: IOError },

    #[snafu(display("Failed to serialize index hunk {:?}", path))]
    SerializeIndex {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[snafu(display("Failed to deserialize index hunk {:?}", path))]
    DeserializeIndex {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[snafu(display("Failed to read metadata file {:?}", path))]
    ReadMetadata {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to write metadata file {:?}", path))]
    WriteMetadata {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to deserialize json from {:?}", path))]
    DeserializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[snafu(display("Failed to serialize json to {:?}", path))]
    SerializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[snafu(display("Failed to list bands in {:?}", path))]
    ListBands {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to read source file {}", path.display()))]
    ReadSourceFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to read source tree {}", path.display()))]
    ListSourceTree { path: PathBuf, source: IOError },

    #[snafu(display("Failed to store file {}", apath))]
    StoreFile { apath: Apath, source: IOError },

    #[snafu(display("Failed to restore {}", path.display()))]
    Restore { path: PathBuf, source: IOError },
}

pub type Result<T> = std::result::Result<T, Error>;
