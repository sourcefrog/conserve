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

use serde::{self, ser::SerializeStruct, Serialize, Serializer};
use thiserror::Error;

use crate::blockdir::Address;
use crate::*;

fn serialize_io_error<S>(err: &io::Error, s: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut t = s.serialize_struct("io::Error", 2)?;
    t.serialize_field("os_error", &err.raw_os_error())?;
    t.serialize_field("message", &err.to_string())?;
    t.end()
}

fn serialize_generic_error<E, S>(err: &E, s: S) -> std::result::Result<S::Ok, S::Error>
where
    E: std::error::Error,
    S: Serializer,
{
    let mut t = s.serialize_struct("Error", 1)?;
    t.serialize_field("message", &err.to_string())?;
    t.end()
}

/// Conserve specific error.
#[non_exhaustive]
#[derive(Debug, Error, Serialize)]
pub enum Error {
    #[error("Block file {hash:?} corrupt; actual hash {actual_hash:?}")]
    BlockCorrupt { hash: String, actual_hash: String },

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
    WriteBlock {
        hash: String,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to read block {hash:?}")]
    ReadBlock {
        hash: String,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Block {block_hash} is missing")]
    BlockMissing { block_hash: BlockHash },

    #[error("Failed to list block files")]
    ListBlocks {
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Not a Conserve archive")]
    NotAnArchive {},

    #[error("Failed to read archive header")]
    ReadArchiveHeader {
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

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

    #[error(
        "Band {band_id} has feature flags {unsupported_flags:?} \
        not supported by Conserve {conserve_version}",
        conserve_version = crate::version()
    )]
    UnsupportedBandFormatFlags {
        band_id: BandId,
        unsupported_flags: Vec<Cow<'static, str>>,
    },

    #[error("Destination directory not empty: {:?}", path)]
    DestinationNotEmpty { path: PathBuf },

    #[error("Archive has no bands")]
    ArchiveEmpty,

    #[error("Directory for new archive is not empty")]
    NewArchiveDirectoryNotEmpty,

    #[error("Invalid backup version number {:?}", version)]
    InvalidVersion { version: String },

    #[error("Failed to create band")]
    CreateBand {
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Band {band_id} head file missing")]
    BandHeadMissing { band_id: BandId },

    #[error("Failed to create block directory")]
    CreateBlockDir {
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to create archive directory")]
    CreateArchiveDirectory {
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

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

    #[error(transparent)]
    ParseGlob {
        #[from]
        #[serde(serialize_with = "serialize_generic_error")]
        source: globset::Error,
    },

    #[error("Failed to write index hunk {:?}", path)]
    WriteIndex {
        path: String,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to read index hunk {:?}", path)]
    ReadIndex {
        path: String,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to serialize index")]
    SerializeIndex {
        #[serde(serialize_with = "serialize_generic_error")]
        source: serde_json::Error,
    },

    #[error("Failed to deserialize index hunk {:?}", path)]
    DeserializeIndex {
        path: String,
        #[serde(serialize_with = "serialize_generic_error")]
        source: serde_json::Error,
    },

    #[error("Failed to write metadata file {:?}", path)]
    WriteMetadata {
        path: String,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to deserialize json from {:?}", path)]
    DeserializeJson {
        path: PathBuf,
        #[serde(serialize_with = "serialize_generic_error")]
        source: serde_json::Error,
    },

    #[error("Failed to serialize json to {:?}", path)]
    SerializeJson {
        path: String,
        #[serde(serialize_with = "serialize_generic_error")]
        source: serde_json::Error,
    },

    #[error("Metadata file not found: {:?}", path)]
    MetadataNotFound {
        path: String,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to list bands")]
    ListBands {
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to read source file {:?}", path)]
    ReadSourceFile {
        path: PathBuf,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to read source tree {:?}", path)]
    ListSourceTree {
        path: PathBuf,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to store file {:?}", apath)]
    StoreFile {
        apath: Apath,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to restore {:?}", path)]
    Restore {
        path: PathBuf,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to restore modification time on {:?}", path)]
    RestoreModificationTime {
        path: PathBuf,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to delete band {}", band_id)]
    BandDeletion {
        band_id: BandId,
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Unsupported URL scheme {:?}", scheme)]
    UrlScheme { scheme: String },

    #[error("Failed to serialize object")]
    SerializeError {
        #[from]
        #[serde(serialize_with = "serialize_generic_error")]
        source: serde_json::Error,
    },

    #[error("Unexpected file {path:?} in archive directory")]
    UnexpectedFile { path: String },

    /// Generic IO error.
    #[error(transparent)]
    IOError {
        #[from]
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
    },

    #[error("Failed to set owner of {path:?}")]
    SetOwner {
        #[serde(serialize_with = "serialize_io_error")]
        source: io::Error,
        path: PathBuf,
    },

    #[error(transparent)]
    SnapCompressionError {
        // TODO: Maybe say in which file, etc.
        #[serde(serialize_with = "serialize_generic_error")]
        #[from]
        source: snap::Error,
    },
}
