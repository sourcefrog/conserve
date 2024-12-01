// Copyright 2024 Martin Pool

//! Leases controlling write access to an archive.

use std::process;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{debug, instrument, trace, warn};
use url::Url;

use crate::transport::{self, Transport, WriteMode};

pub static LEASE_FILENAME: &str = "LEASE";

/// A lease on an archive.
#[derive(Debug)]
pub struct Lease {
    transport: Transport,
    /// URL of the lease file.
    url: Url,
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

    #[error("Existing lease file {url} is corrupt")]
    Corrupt { url: Url },

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
    /// Returns [Error::Busy] or [Error::Corrupt] if the lease is already held by another process.
    #[instrument]
    pub async fn acquire(transport: &Transport) -> Result<Self> {
        let lease_taken = OffsetDateTime::now_utc();
        let lease_expiry = lease_taken + Duration::from_secs(5 * 60);
        let content = LeaseContent {
            host: hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
                .into(),
            pid: Some(process::id()),
            client_version: Some(crate::VERSION.to_string()),
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
                    LeaseState::Held(content) => {
                        return Err(Error::Busy {
                            url,
                            content: Box::new(content),
                        })
                    }
                    LeaseState::Corrupt(_mtime) => {
                        return Err(Error::Corrupt { url });
                    }
                    LeaseState::Free => {
                        debug!("Lease file disappeared after conflict; retrying");
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
            url,
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
    pub async fn peek(transport: &Transport) -> Result<LeaseState> {
        // TODO: Atomically get the content and mtime; that should be one call on s3.
        let metadata = match transport.metadata(LEASE_FILENAME).await {
            Ok(m) => m,
            Err(err) if err.is_not_found() => {
                trace!("lease file not present");
                return Ok(LeaseState::Free);
            }
            Err(err) => {
                warn!(?err, "error getting lease file metadata");
                return Err(err.into());
            }
        };
        let bytes = transport.read(LEASE_FILENAME).await?;
        match serde_json::from_slice(&bytes) {
            Ok(content) => Ok(LeaseState::Held(content)),
            Err(err) => {
                warn!(?err, "error deserializing lease file");
                // We do still at least know that it's held, and when it was taken.
                Ok(LeaseState::Corrupt(metadata.modified))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum LeaseState {
    Free,
    Held(LeaseContent),
    Corrupt(OffsetDateTime),
}

/// Contents of the lease file.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LeaseContent {
    /// Hostname of the client process
    pub host: Option<String>,
    /// Process id of the client.
    pub pid: Option<u32>,
    /// Conserve version string.
    pub client_version: Option<String>,

    /// Time when the lease was taken.
    #[serde(with = "time::serde::iso8601")]
    pub lease_taken: OffsetDateTime,

    /// Unix time after which this lease is stale.
    #[serde(with = "time::serde::iso8601")]
    pub lease_expiry: OffsetDateTime,
}

#[cfg(test)]
mod test {
    use std::fs::{write, File};
    use std::process;

    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn take_lease() {
        let tmp = TempDir::new().unwrap();
        let transport = &Transport::local(tmp.path());
        let lease = Lease::acquire(transport).await.unwrap();
        assert!(tmp.path().join("LEASE").exists());
        assert!(lease.next_renewal > lease.lease_taken);

        let peeked = Lease::peek(transport).await.unwrap();
        let LeaseState::Held(content) = peeked else {
            panic!("lease not held")
        };
        assert_eq!(
            content.host.unwrap(),
            hostname::get().unwrap().to_string_lossy()
        );
        assert_eq!(content.pid, Some(process::id()));

        lease.release().await.unwrap();
        assert!(!tmp.path().join("LEASE").exists());
    }

    #[tokio::test]
    async fn peek_fixed_lease_content() {
        let tmp = TempDir::new().unwrap();
        let transport = &Transport::local(tmp.path());
        write(
            tmp.path().join("LEASE"),
            r#"
        {
            "host": "somehost",
            "pid": 1234,
            "client_version": "0.1.2",
            "lease_taken": "2021-01-01T12:34:56Z",
            "lease_expiry": "2021-01-01T12:35:56Z"
        }"#,
        )
        .unwrap();
        let state = Lease::peek(transport).await.unwrap();
        dbg!(&state);
        match state {
            LeaseState::Held(content) => {
                assert_eq!(content.host.unwrap(), "somehost");
                assert_eq!(content.pid, Some(1234));
                assert_eq!(content.client_version.unwrap(), "0.1.2");
                assert_eq!(content.lease_taken.year(), 2021);
                assert_eq!(content.lease_expiry.year(), 2021);
                assert_eq!(
                    content.lease_expiry - content.lease_taken,
                    time::Duration::seconds(60)
                );
            }
            _ => panic!("lease should be recognized as held, got {state:?}"),
        }
    }

    /// An empty lease file is judged by its mtime; the lease can be grabbed a while
    /// after it was last written.
    #[tokio::test]
    async fn peek_corrupt_empty_lease() {
        let tmp = TempDir::new().unwrap();
        let transport = &Transport::local(tmp.path());
        File::create(tmp.path().join("LEASE")).unwrap();
        let state = Lease::peek(transport).await.unwrap();
        match state {
            LeaseState::Corrupt(mtime) => {
                let now = time::OffsetDateTime::now_utc();
                assert!(now - mtime < time::Duration::seconds(15));
            }
            _ => panic!("lease should be recognized as corrupt, got {state:?}"),
        }
    }
}
