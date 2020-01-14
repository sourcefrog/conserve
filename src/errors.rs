// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Conserve error types.

use std::path::PathBuf;

use snafu::Snafu;

use crate::*;

type IOError = std::io::Error;

/// Conserve specific error.
// TODO: Perhaps have an enum per module?
#[derive(Debug, Snafu)]
#[snafu(visibility = "pub(crate)")]
pub enum Error {
    BlockCorrupt {
        path: PathBuf,
    },
    WriteBlockFile {
        source: IOError,
    },
    PersistBlockFile {
        source: tempfile::PersistError,
    },
    ReadBlock {
        source: IOError,
    },
    ListBlocks {
        source: IOError,
    },
    #[snafu(display("Not a Conserve archive: {}", path.display()))]
    NotAnArchive {
        path: PathBuf,
    },
    NotADirectory {
        path: PathBuf,
    },
    NotAFile {
        path: PathBuf,
    },
    UnsupportedArchiveVersion {
        version: String,
    },
    #[snafu(display("Destination directory not empty: {}", path.display()))]
    DestinationNotEmpty {
        path: PathBuf,
    },
    ArchiveEmpty,
    #[snafu(display("Archive has no complete bands"))]
    NoCompleteBands,

    #[snafu(display("Invalid version {:?}", version))]
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
    MeasureBandSize {
        source: walkdir::Error,
    },
    ReadSourceFile {
        path: PathBuf,
        source: std::io::Error,
    },
    ListSourceTree {
        path: PathBuf,
        source: IOError,
    },
    StoreFile {
        source: IOError,
    },
    Restore {
        path: PathBuf,
        source: IOError,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
