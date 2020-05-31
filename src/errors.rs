// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Conserve error types.

use std::path::PathBuf;

use thiserror::Error;

use crate::*;

type IOError = std::io::Error;

/// Conserve specific error.
#[derive(Debug, Error)]
pub enum Error {
    // TODO: Add messages and perhaps more fields to all of these.
    #[error("Block file {path:?} corrupt; actual hash {actual_hash:?}")]
    BlockCorrupt { path: PathBuf, actual_hash: String },

    #[error("Failed to store block {block_hash:?}")]
    StoreBlock { block_hash: String, source: IOError },

    #[error("Failed to read block {path:?}")]
    ReadBlock { path: PathBuf, source: IOError },

    #[error("Failed to list block files in {:?}", path)]
    ListBlocks { path: PathBuf, source: IOError },

    #[error("Not a Conserve archive: {:?}", path)]
    NotAnArchive { path: PathBuf },

    #[error("Failed to read archive header from {:?}", path)]
    ReadArchiveHeader {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error(
        "Archive version {:?} in {:?} is not supported by Conserve {}",
        version,
        path,
        crate::version()
    )]
    UnsupportedArchiveVersion { path: PathBuf, version: String },

    #[error(
        "Band version {:?} in {:?} is not supported by Conserve {}",
        version,
        path,
        crate::version()
    )]
    UnsupportedBandVersion { path: PathBuf, version: String },

    #[error("Destination directory not empty: {:?}", path)]
    DestinationNotEmpty { path: PathBuf },

    #[error("Archive has no bands")]
    ArchiveEmpty,

    #[error("Invalid backup version number {:?}", version)]
    InvalidVersion { version: String },

    #[error("Failed to create band")]
    CreateBand { source: std::io::Error },

    #[error("Failed to create block directory")]
    CreateBlockDir { source: std::io::Error },

    #[error("Failed to create archive directory {:?}", path)]
    CreateArchiveDirectory {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Band {} is incomplete", band_id)]
    BandIncomplete { band_id: BandId },

    #[error(transparent)]
    ParseGlob {
        #[from]
        source: globset::Error,
    },

    #[error("Failed to write index hunk {:?}", path)]
    WriteIndex { path: PathBuf, source: IOError },

    #[error("Failed to read index hunk {:?}", path)]
    ReadIndex { path: PathBuf, source: IOError },

    #[error("Failed to serialize index hunk {:?}", path)]
    SerializeIndex {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to deserialize index hunk {:?}", path)]
    DeserializeIndex {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to read metadata file {:?}", path)]
    ReadMetadata {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to write metadata file {:?}", path)]
    WriteMetadata {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to deserialize json from {:?}", path)]
    DeserializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to serialize json to {:?}", path)]
    SerializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to list bands in {:?}", path)]
    ListBands {
        path: PathBuf,
        source: std::io::Error,
    },

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

    /// Generic string error.
    #[error(transparent)]
    Other {
        #[from]
        source: anyhow::Error,
    },
}

pub type Result<T> = anyhow::Result<T, Error>;
