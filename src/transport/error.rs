// Copyright 2020-2025 Martin Pool.

//! Errors occurring on Transports: reading, writing, or listing files.

use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::path::Path;

use derive_more::Display;
use url::Url;

/// A transport error, as a generalization of IO errors.
#[derive(Debug)]
pub struct Error {
    /// What type of generally known error?
    pub kind: ErrorKind,
    /// The underlying error: for example an IO or S3 error.
    pub source: Option<Box<dyn StdError + Send + Sync>>,
    /// The affected URL, if known.
    pub url: Option<Url>,
}

/// General categories of transport errors.
#[derive(Debug, Display, PartialEq, Eq, Clone, Copy)]
pub enum ErrorKind {
    #[display(fmt = "Not found")]
    NotFound,

    #[display(fmt = "Already exists")]
    AlreadyExists,

    #[display(fmt = "Permission denied")]
    PermissionDenied,

    #[display(fmt = "Create transport error")]
    CreateTransport,

    #[display(fmt = "Connect error")]
    Connect,

    #[display(fmt = "Unsupported URL scheme")]
    UrlScheme,

    #[display(fmt = "Other transport error")]
    Other,
}

impl From<io::ErrorKind> for ErrorKind {
    fn from(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => ErrorKind::NotFound,
            io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Other,
        }
    }
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub(super) fn io_error(path: &Path, source: io::Error) -> Error {
        let kind = match source.kind() {
            io::ErrorKind::NotFound => ErrorKind::NotFound,
            io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Other,
        };

        Error {
            source: Some(Box::new(source)),
            url: Url::from_file_path(path).ok(),
            kind,
        }
    }

    pub fn is_not_found(&self) -> bool {
        self.kind == ErrorKind::NotFound
    }

    /// The URL where this error occurred, if known.
    pub fn url(&self) -> Option<&Url> {
        self.url.as_ref()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(ref url) = self.url {
            write!(f, ": {url}")?;
        }
        if let Some(source) = &self.source {
            // I'm not sure we should write this here; it might be repetitive.
            write!(f, ": {source}")?;
        }
        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_ref().map(|s| &**s as _)
    }
}
