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

//! Conserve error types.

use std::borrow::Cow;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::blockdir::Address;
use crate::*;

/// Conserve specific error.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    #[error("Block file {hash:?} corrupt: does not have the expected hash")]
    BlockCorrupt { hash: BlockHash },

    #[error("{address:?} extends beyond decompressed block length {actual_len:?}")]
    AddressTooLong { address: Address, actual_len: usize },

    // TODO: Merge with AddressTooLong
    #[error(
        "block {block_hash} actual length is {actual_len} but indexes reference {referenced_len}"
    )]
    ShortBlock {
        block_hash: BlockHash,
        actual_len: usize,
        referenced_len: u64,
    },

    #[error("Failed to write block {hash:?}")]
    WriteBlock { hash: String, source: io::Error },

    #[error("Failed to read block {hash:?}")]
    ReadBlock { hash: String, source: io::Error },

    #[error("Block {block_hash} is missing")]
    BlockMissing { block_hash: BlockHash },

    #[error("Failed to list block files")]
    ListBlocks { source: io::Error },

    #[error("Not a Conserve archive (no CONSERVE header found)")]
    NotAnArchive,

    #[error("Failed to read archive header")]
    ReadArchiveHeader { source: io::Error },

    #[error(
        "Archive version {:?} is not supported by Conserve {}",
        version,
        crate::version()
    )]
    UnsupportedArchiveVersion { version: String },

    #[error("Unsupported band version {version:?} in {band_id}")]
    UnsupportedBandVersion { band_id: BandId, version: String },

    #[error("Archive is empty")]
    ArchiveEmpty,

    #[error("Archive has no complete bands")]
    NoCompleteBands,

    #[error("Unsupported band format flags {unsupported_flags:?} in {band_id}")]
    UnsupportedBandFormatFlags {
        band_id: BandId,
        unsupported_flags: Vec<Cow<'static, str>>,
    },

    #[error("Destination directory is not empty")]
    DestinationNotEmpty,

    #[error("Directory for new archive is not empty")]
    NewArchiveDirectoryNotEmpty,

    #[error("Invalid backup version number {:?}", version)]
    InvalidVersion { version: String },

    #[error("Failed to create band")]
    CreateBand { source: io::Error },

    #[error("Band {band_id} head file missing")]
    BandHeadMissing { band_id: BandId },

    #[error("Failed to create block directory")]
    CreateBlockDir { source: io::Error },

    #[error("Failed to create archive directory")]
    CreateArchiveDirectory { source: io::Error },

    #[error("Band {} is incomplete", band_id)]
    BandIncomplete { band_id: BandId },

    #[error("Duplicated band directory for {band_id}")]
    DuplicateBandDirectory { band_id: BandId },

    #[error(
        "Can't delete blocks because the last band ({}) is incomplete and may be in use",
        band_id
    )]
    DeleteWithIncompleteBackup { band_id: BandId },

    #[error("Can't continue with deletion because the archive was changed by another process")]
    DeleteWithConcurrentActivity,

    #[error("Archive is locked for garbage collection")]
    GarbageCollectionLockHeld,

    #[error("A backup was created while the garbage collection lock was held; CHECK ARCHIVE NOW")]
    GarbageCollectionLockHeldDuringBackup,

    #[error(transparent)]
    ParseGlob {
        #[from]
        source: globset::Error,
    },

    #[error("Failed to write index hunk {:?}", path)]
    WriteIndex { path: String, source: io::Error },

    #[error("Failed to read index hunk {:?}", path)]
    ReadIndex { path: String, source: io::Error },

    #[error("Failed to serialize index")]
    SerializeIndex { source: serde_json::Error },

    #[error("Failed to deserialize index hunk {:?}", path)]
    DeserializeIndex {
        path: String,
        source: serde_json::Error,
    },

    #[error("Failed to write metadata file {:?}", path)]
    WriteMetadata { path: String, source: io::Error },

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

    #[error("Metadata file not found: {:?}", path)]
    MetadataNotFound { path: String, source: io::Error },

    #[error("Invalid metadata: {details}")]
    InvalidMetadata { details: String },

    #[error("Band not found: {band_id}")]
    BandNotFound { band_id: BandId },

    #[error("Failed to list bands")]
    ListBands { source: io::Error },

    #[error("Failed to read source file {:?}", path)]
    ReadSourceFile { path: PathBuf, source: io::Error },

    #[error("Unsupported source file kind: {path:?}")]
    UnsupportedSourceKind { path: PathBuf },

    #[error("Unsupported symlink encoding: {path:?}")]
    UnsupportedTargetEncoding { path: PathBuf },

    #[error("Failed to read source tree {:?}", path)]
    ListSourceTree { path: PathBuf, source: io::Error },

    #[error("Failed to store file {:?}", apath)]
    StoreFile { apath: Apath, source: io::Error },

    #[error("Failed to restore {:?}", path)]
    Restore { path: PathBuf, source: io::Error },

    #[error("Failed to restore modification time on {:?}", path)]
    RestoreModificationTime { path: PathBuf, source: io::Error },

    #[error("Unsupported URL scheme {:?}", scheme)]
    UrlScheme { scheme: String },

    #[error("Failed to serialize object")]
    SerializeError {
        #[from]
        source: serde_json::Error,
    },

    #[error("Unexpected file {path:?} in archive directory")]
    UnexpectedFile { path: String },

    /// Generic IO error.
    #[error(transparent)]
    IOError {
        #[from]
        source: io::Error,
    },

    #[error("Failed to set owner of {path:?}")]
    SetOwner { source: io::Error, path: PathBuf },

    #[error(transparent)]
    SnapCompressionError {
        // TODO: Maybe say in which file, etc.
        #[from]
        source: snap::Error,
    },

    #[error(transparent)]
    Transport {
        #[from]
        source: transport::Error,
    },
}

impl From<jsonio::Error> for Error {
    fn from(value: jsonio::Error) -> Self {
        match value {
            jsonio::Error::Io { source } => Error::IOError { source },
            jsonio::Error::Json { source, path } => Error::DeserializeJson { source, path }, // conflates serialize/deserialize
            jsonio::Error::Transport { source } => Error::Transport { source },
        }
    }
}
