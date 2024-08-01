// Conserve backup system.
// Copyright 2015-2024 Martin Pool.

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

use crate::*;

/// Conserve specific error.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    #[error("Block file {hash:?} corrupt: does not have the expected hash")]
    BlockCorrupt { hash: BlockHash },

    #[error("Referenced block {hash} is missing")]
    BlockMissing { hash: BlockHash },

    #[error("Block {hash} is too short: actual len {actual_len}, referenced len {referenced_len}")]
    BlockTooShort {
        hash: BlockHash,
        actual_len: usize,
        referenced_len: usize,
    },

    #[error("Failed to list blocks")]
    ListBlocks {
        #[source]
        source: transport::Error,
    },

    #[error("Not a Conserve archive (no CONSERVE header found)")]
    NotAnArchive,

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

    #[error("Band {band_id} head file missing")]
    BandHeadMissing { band_id: BandId },

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

    #[error("Failed to deserialize json from {:?}", path)]
    DeserializeJson {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("Failed to serialize json")]
    SerializeJson {
        #[from]
        source: serde_json::Error,
    },

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

    #[error("Failed to restore file {:?}", path)]
    RestoreFile { path: PathBuf, source: io::Error },

    #[error("Failed to restore symlink {path:?}")]
    RestoreSymlink { path: PathBuf, source: io::Error },

    #[error("Failed to read block content {hash} for {apath}")]
    RestoreFileBlock {
        apath: Apath,
        hash: BlockHash,
        source: Box<Error>,
    },

    #[error("Failed to restore directory {:?}", path)]
    RestoreDirectory { path: PathBuf, source: io::Error },

    #[error("Failed to restore ownership of {:?}", path)]
    RestoreOwnership { path: PathBuf, source: io::Error },

    #[error("Failed to restore permissions on {:?}", path)]
    RestorePermissions { path: PathBuf, source: io::Error },

    #[error("Failed to restore modification time on {:?}", path)]
    RestoreModificationTime { path: PathBuf, source: io::Error },

    #[error("Unsupported URL scheme {:?}", scheme)]
    UrlScheme { scheme: String },

    #[error("Unexpected file {path:?} in archive directory")]
    UnexpectedFile { path: String },

    #[error("This feature is not implemented")]
    NotImplemented,

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

    #[cfg(windows)]
    #[error(transparent)]
    Projection {
        #[from]
        source: windows_projfs::Error,
    },
}

impl From<jsonio::Error> for Error {
    fn from(value: jsonio::Error) -> Self {
        match value {
            jsonio::Error::Io { source } => Error::IOError { source },
            jsonio::Error::Json { source, path } => Error::DeserializeJson {
                source,
                path: path.to_string_lossy().into_owned(),
            }, // conflates serialize/deserialize
            jsonio::Error::Transport { source } => Error::Transport { source },
        }
    }
}
