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
    // TODO: Add `backtrace` members on errors that are more likely to be
    // internal errors or have mysterious tracebacks.
    BlockCorrupt {
        path: PathBuf,
    },
    WriteBlockFile {
        source: IOError,
    },
    PersistBlockFile {
        source: tempfile::PersistError,
    },

    #[snafu(display("Error reading block {:?}", path))]
    ReadBlock {
        path: PathBuf,
        source: IOError,
    },

    ListBlocks {
        source: IOError,
    },

    #[snafu(display("Not a Conserve archive: {}", path.display()))]
    NotAnArchive {
        path: PathBuf,
    },

    #[snafu(display("Failed to read archive header {}", path.display()))]
    ReadArchiveHeader {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Archive version {:?} in {} is not supported by Conserve {}",
        version, path.display(), crate::version()))]
    UnsupportedArchiveVersion {
        path: PathBuf,
        version: String,
    },

    #[snafu(display("Destination directory not empty: {}", path.display()))]
    DestinationNotEmpty {
        path: PathBuf,
    },
    ArchiveEmpty,
    #[snafu(display("Archive has no complete bands"))]
    NoCompleteBands,

    #[snafu(display("Invalid backup version number {:?}", version))]
    InvalidVersion {
        version: String,
    },

    CreateBand {
        source: std::io::Error,
    },
    CreateBlockDir {
        source: std::io::Error,
    },
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    BandIncomplete {
        band_id: BandId,
    },
    ParseGlob {
        source: globset::Error,
    },
    IndexCorrupt {
        path: PathBuf,
    },
    WriteIndex {
        source: IOError,
    },
    ReadIndex {
        source: IOError,
    },
    SerializeIndex {
        source: serde_json::Error,
    },
    DeserializeIndex {
        path: PathBuf,
        source: serde_json::Error,
    },
    FileCorrupt {
        // band_id: BandId,
        apath: Apath,
        expected_hex: String,
        actual_hex: String,
    },

    #[snafu(display("Failed to read metadata file {}", path.display()))]
    ReadMetadata {
        path: PathBuf,
        source: std::io::Error,
    },

    DeserializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    WriteMetadata {
        path: PathBuf,
        source: std::io::Error,
    },
    SerializeJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    ListBands {
        path: PathBuf,
        source: std::io::Error,
    },

    ReadSourceFile {
        path: PathBuf,
        source: std::io::Error,
    },

    ListSourceTree {
        path: PathBuf,
        source: IOError,
    },

    #[snafu(display("Error storing file {}", apath))]
    StoreFile {
        apath: Apath,
        source: IOError,
    },

    Restore {
        path: PathBuf,
        source: IOError,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
