// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Conserve error types.

use std::error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::*;

/// Conserve specific error.
#[derive(Debug)]
pub enum Error {
    BlockCorrupt(PathBuf),
    NotAnArchive(PathBuf),
    NotADirectory(PathBuf),
    NotAFile(PathBuf),
    UnsupportedArchiveVersion(String),
    DestinationNotEmpty(PathBuf),
    ArchiveEmpty,
    NoCompleteBands,
    InvalidVersion,
    BandIncomplete(BandId),
    IoError(io::Error),
    // TODO: Include the path in the json error.
    JsonDeserialize(serde_json::Error),
    BadGlob(globset::Error),
    IndexCorrupt(PathBuf),
    CrossTerm(crossterm::ErrorKind),
    FileCorrupt {
        // band_id: BandId,
        apath: Apath,
        expected_hex: String,
        actual_hex: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::DestinationNotEmpty(d) => write!(f, "Destination directory not empty: {:?}", d),
            Error::ArchiveEmpty => write!(f, "Archive is empty"),
            Error::NoCompleteBands => write!(f, "Archive has no complete bands"),
            Error::InvalidVersion => write!(f, "Invalid version number"),
            Error::NotAnArchive(p) => write!(f, "Not a Conserve archive: {:?}", p),
            Error::BandIncomplete(b) => write!(f, "Band {} is incomplete", b),
            Error::UnsupportedArchiveVersion(v) => write!(
                f,
                "Archive version {:?} is not supported by Conserve {}",
                v,
                version()
            ),
            Error::IoError(e) => write!(f, "IO Error: {}", e),
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            // For cases like IoError that essentially include an underlying
            // error by value, it doesn't seem to help anything, and tends to
            // cause doubled-up messages, to treat them as a separate cause.
            //
            // For now, I'll reserve this for cases where the conserve error
            // is abstracted from its cause.
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(c: io::Error) -> Error {
        Error::IoError(c)
    }
}

impl From<globset::Error> for Error {
    fn from(c: globset::Error) -> Error {
        Error::BadGlob(c)
    }
}

impl From<serde_json::Error> for Error {
    fn from(c: serde_json::Error) -> Error {
        Error::JsonDeserialize(c)
    }
}

impl From<crossterm::ErrorKind> for Error {
    fn from(c: crossterm::ErrorKind) -> Error {
        Error::CrossTerm(c)
    }
}
