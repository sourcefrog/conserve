// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Conserve error types.

use std::error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use rustc_serialize;

use super::*;

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
    JsonDecode(rustc_serialize::json::DecoderError),
    BadGlob(globset::Error),
    IndexCorrupt(PathBuf),
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
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(error::Error + 'static)> {
        match self {
            Error::IoError(c) => Some(c),
            Error::JsonDecode(c) => Some(c),
            Error::BadGlob(c) => Some(c),
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

impl From<rustc_serialize::json::DecoderError> for Error {
    fn from(c: rustc_serialize::json::DecoderError) -> Error {
        Error::JsonDecode(c)
    }
}
