// Copyright 2024 Martin Pool

//! Leases controlling write access to an archive.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::jsonio::{self, read_json};
use crate::transport::{self, Transport};

pub static LEASE_FILENAME: &str = "LEASE.json";

/// A lease on an archive.
#[derive(Debug)]
pub struct Lease {
    _transport: Arc<Transport>,
}

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    #[error("Lease {url} is held by another process: {content:?}")]
    Busy { url: Url, content: LeaseContent },

    #[error("Transport error on lease file: {source}")]
    Transport {
        #[from]
        source: transport::Error,
    },

    #[error("JSON serialization error in lease {url}: {source}")]
    Json { source: serde_json::Error, url: Url },
}

type Result<T> = std::result::Result<T, Error>;

impl Lease {
    /// Acquire a lease. If it's already held by some other process, wait for it to be ready.
    pub fn acquire(_transport: Arc<Transport>) -> Result<Self> {
        todo!()
    }

    /// Return information about the current leaseholder, if any.
    pub fn peek(transport: Arc<Transport>) -> Result<Option<LeaseContent>> {
        read_json(&transport, LEASE_FILENAME).map_err(|err| match err {
            jsonio::Error::Transport { source, .. } => Error::Transport { source },
            jsonio::Error::Json { source, .. } => Error::Json {
                source,
                url: transport.url().join(LEASE_FILENAME).unwrap(),
            },
        })
    }
}

/// Contents of the lease file.
#[derive(Debug, Serialize, Deserialize)]
pub struct LeaseContent {
    /// Hostname of the client process
    pub host: String,
    /// Process id of the client.
    pub pid: u32,
    /// Conserve version string.
    pub client_version: String,
    /// Time when the lease was taken.
    pub lease_taken: u32,
    /// Unix time after which this lease is stale.
    pub lease_expiry: u32,
}

#[cfg(test)]
mod test {}
