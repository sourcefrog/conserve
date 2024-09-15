// Copyright 2024 Martin Pool

//! Leases controlling write access to an archive.

use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{debug, instrument};
use url::Url;

use crate::{
    jsonio::{self, read_json},
    transport, Transport,
};

pub static LEASE_FILENAME: &str = "LEASE.json";

/// A lease on an archive.
#[derive(Debug)]
pub struct Lease {
    transport: Arc<dyn Transport>,
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
    pub fn acquire(transport: Arc<dyn Transport>) -> Result<Self> {
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
        let url = transport.relative_file_url(LEASE_FILENAME);
        let mut s: String = serde_json::to_string(&content).expect("serialize lease");
        s.push('\n');
        while let Err(err) = transport
            .as_ref()
            .write_new_file(LEASE_FILENAME, s.as_bytes())
        {
            if err.kind() == transport::ErrorKind::AlreadyExists {
                match Lease::peek(transport.as_ref())? {
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
            transport,
            lease_taken,
            next_renewal,
        })
    }

    #[instrument]
    pub fn release(self) -> Result<()> {
        // TODO: Check that it was not stolen?
        self.transport
            .as_ref()
            .remove_file(LEASE_FILENAME)
            .map_err(Error::from)
    }

    /// Return information about the current leaseholder, if any.
    pub fn peek(transport: &dyn Transport) -> Result<Option<LeaseContent>> {
        read_json(transport, LEASE_FILENAME).map_err(|err| match err {
            jsonio::Error::Transport { source, .. } => Error::Transport { source },
            jsonio::Error::Json { source, .. } => Error::Json {
                source,
                url: transport.base_url().join(LEASE_FILENAME).unwrap(),
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

    use crate::transport::open_local_transport;

    use super::Lease;

    #[test]
    fn take_lease() {
        let tmp = TempDir::new().unwrap();
        let transport = open_local_transport(tmp.path()).unwrap();
        let lease = Lease::acquire(transport.clone()).unwrap();
        assert!(tmp.path().join("LEASE.json").exists());
        assert!(lease.next_renewal > lease.lease_taken);

        let peeked = Lease::peek(transport.as_ref()).unwrap().unwrap();
        assert_eq!(peeked.host, hostname::get().unwrap().to_string_lossy());
        assert_eq!(peeked.pid, std::process::id());

        lease.release().unwrap();
        assert!(!tmp.path().join("LEASE.json").exists());
    }
}
