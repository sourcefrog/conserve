// Conserve backup system.
// Copyright 2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Conserve Transport error types.
//!
//! These are like [std::io::Error], but abstracted to handle writing
//! to object stores like S3.

// use std::borrow::Cow;
use std::io;
use std::path::Path;

// use serde::Serialize;
use thiserror::Error;
use url::Url;

/// Conserve specific error.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    #[error("Not found: {url}")]
    NotFound { url: Url },

    #[error("Transport IO error")]
    OtherIoError { source: io::Error },
}

impl Error {
    pub fn io_error(path: &Path, source: io::Error) -> Self {
        match source.kind() {
            io::ErrorKind::NotFound => Error::NotFound {
                url: Url::from_file_path(path).expect("Convert path to URL"),
            },
            _ => Error::OtherIoError { source },
        }
    }
}
