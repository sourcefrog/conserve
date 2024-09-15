// Copyright 2024 Martin Pool

//! Leases controlling write access to an archive.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{debug, instrument};
use url::Url;

use crate::jsonio::{self, read_json};
use crate::transport::{self, Transport, WriteMode};

pub static LEASE_FILENAME: &str = "LEASE";

/// A lease on an archive.
#[derive(Debug)]
pub struct Lease {
    transport: Transport,
    lease_taken: OffsetDateTime,
    /// The next refresh after this time must rewrite the lease.
    next_renewal: OffsetDateTime,
}

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    #[error("Lease {url} is held by another process: {content:?}")]
    Busy {
        url: Url,
        content: Box<LeaseContent>,
    },

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
    /// Acquire a lease, if one is available.
    ///
    /// Returns [Error::Busy] if the lease is already held by another process.
    #[instrument]
    pub async fn acquire(transport: &Transport) -> Result<Self> {
        let lease_taken = OffsetDateTime::now_utc();
        let lease_expiry = lease_taken + Duration::from_secs(5 * 60);
        let content = LeaseContent {
            host: hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            pid: std::process::id(),
            client_version: crate::VERSION.to_string(),
            lease_taken,
            lease_expiry,
        };
        let url = transport.url().join(LEASE_FILENAME).unwrap();
        let mut s: String = serde_json::to_string(&content).expect("serialize lease");
        s.push('\n');
        while let Err(err) = transport
            .write(LEASE_FILENAME, s.as_bytes(), WriteMode::CreateNew)
            .await
        {
            if err.kind() == transport::ErrorKind::AlreadyExists {
                match Lease::peek(transport).await? {
                    Some(content) => {
                        return Err(Error::Busy {
                            url,
                            content: Box::new(content),
                        })
                    }
                    None => {
                        debug!("Lease file disappeared after conflict");
                        continue;
                    }
                }
            } else {
                return Err(err.into());
            }
        }
        let next_renewal = lease_taken + Duration::from_secs(60);
        Ok(Lease {
            transport: transport.clone(),
            lease_taken,
            next_renewal,
        })
    }

    #[instrument]
    pub async fn release(self) -> Result<()> {
        // TODO: Check that it was not stolen?
        self.transport
            .remove_file(LEASE_FILENAME)
            .await
            .map_err(Error::from)
    }

    /// Return information about the current leaseholder, if any.
    pub async fn peek(transport: &Transport) -> Result<Option<LeaseContent>> {
        read_json(transport, LEASE_FILENAME)
            .await
            .map_err(|err| match err {
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
    #[serde(with = "time::serde::iso8601")]
    pub lease_taken: OffsetDateTime,

    /// Unix time after which this lease is stale.
    #[serde(with = "time::serde::iso8601")]
    pub lease_expiry: OffsetDateTime,
}

#[cfg(test)]
mod test {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn take_lease() {
        let tmp = TempDir::new().unwrap();
        let transport = &Transport::local(tmp.path());
        let lease = Lease::acquire(transport).await.unwrap();
        assert!(tmp.path().join("LEASE").exists());
        assert!(lease.next_renewal > lease.lease_taken);

        let peeked = Lease::peek(transport).await.unwrap().unwrap();
        assert_eq!(peeked.host, hostname::get().unwrap().to_string_lossy());
        assert_eq!(peeked.pid, std::process::id());

        lease.release().await.unwrap();
        assert!(!tmp.path().join("LEASE").exists());
    }
}
