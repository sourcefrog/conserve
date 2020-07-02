// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Conserve error types.

use std::path::PathBuf;

use thiserror::Error;

use crate::blockdir::Address;
use crate::*;

type IOError = std::io::Error;

/// Conserve specific error.
#[derive(Debug, Error)]
pub enum Error {
    #[error("Block file {hash:?} corrupt; actual hash {actual_hash:?}")]
    BlockCorrupt { hash: String, actual_hash: String },

    #[error("{address:?} extends beyond decompressed block length {actual_len:?}")]
    AddressTooLong { address: Address, actual_len: usize },

    #[error("{address:?} in {apath} in {band_id} extends beyond decompressed block length {block_len:?}")]
    AddressTooLongInFile {
        address: Address,
        apath: Apath,
        band_id: BandId,
        block_len: usize,
    },

    #[error("{address:?} in {apath} in {band_id} references missing block")]
    BlockMissingInFile {
        address: Address,
        apath: Apath,
        band_id: BandId,
    },

    #[error("Failed to write block {hash:?}")]
    WriteBlock { hash: String, source: IOError },

    #[error("Failed to read block {hash:?}")]
    ReadBlock { hash: String, source: IOError },

    #[error("Failed to list block files")]
    ListBlocks { source: IOError },

    #[error("Not a Conserve archive")]
    NotAnArchive {},

    #[error("Failed to read archive header")]
    ReadArchiveHeader { source: std::io::Error },

    #[error(
        "Archive version {:?} is not supported by Conserve {}",
        version,
        crate::version()
    )]
    UnsupportedArchiveVersion { version: String },

    #[error(
        "Band version {version:?} in {band_id} is not supported by Conserve {}",
        crate::version()
    )]
    UnsupportedBandVersion { band_id: BandId, version: String },

    #[error("Destination directory not empty: {:?}", path)]
    DestinationNotEmpty { path: PathBuf },

    #[error("Archive has no bands")]
    ArchiveEmpty,

    #[error("Directory for new archive is not empty")]
    NewArchiveDirectoryNotEmpty,

    #[error("Invalid backup version number {:?}", version)]
    InvalidVersion { version: String },

    #[error("Failed to create band")]
    CreateBand { source: std::io::Error },

    #[error("Failed to create block directory")]
    CreateBlockDir { source: std::io::Error },

    #[error("Failed to create archive directory")]
    CreateArchiveDirectory { source: std::io::Error },

    #[error("Band {} is incomplete", band_id)]
    BandIncomplete { band_id: BandId },

    #[error(transparent)]
    ParseGlob {
        #[from]
        source: globset::Error,
    },

    #[error("Failed to write index hunk {:?}", path)]
    WriteIndex { path: String, source: IOError },

    #[error("Failed to read index hunk {:?}", path)]
    ReadIndex { path: String, source: IOError },

    #[error("Failed to serialize index")]
    SerializeIndex { source: serde_json::Error },

    #[error("Failed to deserialize index hunk {:?}", path)]
    DeserializeIndex {
        path: String,
        source: serde_json::Error,
    },

    #[error("Failed to write metadata file {:?}", path)]
    WriteMetadata {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to deserialize json from {:?}", path)]
    DeserializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to serialize json to {:?}", path)]
    SerializeJson {
        path: String,
        source: serde_json::Error,
    },

    #[error("Failed to list bands")]
    ListBands { source: std::io::Error },

    #[error("Failed to read source file {:?}", path)]
    ReadSourceFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to read source tree {:?}", path)]
    ListSourceTree { path: PathBuf, source: IOError },

    #[error("Failed to store file {:?}", apath)]
    StoreFile { apath: Apath, source: IOError },

    #[error("Failed to restore {:?}", path)]
    Restore { path: PathBuf, source: IOError },

    /// Generic IO error.
    #[error(transparent)]
    IOError {
        #[from]
        source: IOError,
    },

    #[error(transparent)]
    SnapCompressionError {
        #[from]
        source: snap::Error,
    },
}
