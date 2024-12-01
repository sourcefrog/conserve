// Copyright 2024 Martin Pool

//! Leases controlling write access to an archive.

use std::process;
use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{debug, instrument, trace, warn};
use url::Url;

use crate::{transport, Transport};

pub static LEASE_FILENAME: &str = "LEASE";

/// A lease on an archive.
#[derive(Debug)]
pub struct Lease {
    transport: Arc<dyn Transport>,
    content: LeaseContent,
    /// The next refresh after this time must rewrite the lease.
    next_renewal: OffsetDateTime,
    /// How often should we renew the lease?
    renewal_interval: Duration,
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

    #[error("Lease {url} was stolen: {content:?}")]
    Stolen {
        url: Url,
        content: Box<LeaseContent>,
    },

    #[error("Lease {url} disappeared")]
    Disappeared { url: Url },
}

type Result<T> = std::result::Result<T, Error>;

/// Options controlling lease behavior, exposed for testing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseOptions {
    /// How long do leases last before they're assumed stale?
    lease_expiry: Duration,

    /// Renew the lease soon after it becomes this old.
    renewal_interval: Duration,
}

impl Default for LeaseOptions {
    fn default() -> Self {
        Self {
            lease_expiry: Duration::from_secs(60),
            renewal_interval: Duration::from_secs(10),
        }
    }
}

impl Lease {
    /// Acquire a lease, if one is available.
    ///
    /// Returns [Error::Busy] or [Error::Corrupt] if the lease is already held by another process.
    pub fn try_acquire(
        transport: Arc<dyn Transport>,
        lease_options: &LeaseOptions,
    ) -> Result<Self> {
        trace!("trying to acquire lease");
        let lease_taken = OffsetDateTime::now_utc();
        let lease_expiry = lease_taken + lease_options.lease_expiry;
        let content = LeaseContent {
            host: hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
                .into(),
            pid: Some(process::id()),
            client_version: Some(crate::VERSION.to_string()),
            acquired: lease_taken,
            expiry: lease_expiry,
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
            transport,
            content,
            next_renewal,
            renewal_interval: lease_options.renewal_interval,
        })
    }

    /// Unconditionally renew a held lease, after checking that it was not stolen.
    ///
    /// This takes the existing lease and returns a new one only if renewal succeeds.
    pub fn renew(mut self) -> Result<Self> {
        let state = Lease::peek(self.transport.as_ref())?;
        let url = self.transport.relative_file_url(LEASE_FILENAME);
        match state {
            LeaseState::Held(content) => {
                if content != self.content {
                    warn!(actual = ?content, expected = ?self.content, "lease stolen");
                    return Err(Error::Stolen {
                        url,
                        content: Box::new(content),
                    });
                }
            }
            LeaseState::Free => {
                warn!("lease file disappeared");
                return Err(Error::Disappeared { url });
            }
            LeaseState::Corrupt(_mtime) => {
                warn!("lease file is corrupt");
                return Err(Error::Corrupt { url });
            }
        }
        self.content.acquired = OffsetDateTime::now_utc();
        self.next_renewal = self.content.acquired + self.renewal_interval;
        let json: String = serde_json::to_string(&self.content).expect("serialize lease");
        self.transport.write_file(LEASE_FILENAME, json.as_bytes())?;
        Ok(self)
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
    pub fn peek(transport: &dyn Transport) -> Result<LeaseState> {
        // TODO: Atomically get the content and mtime; that should be one call on s3.
        let metadata = match transport.metadata(LEASE_FILENAME) {
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
        let bytes = transport.read_file(LEASE_FILENAME)?;
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
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct LeaseContent {
    /// Hostname of the client process
    pub host: Option<String>,
    /// Process id of the client.
    pub pid: Option<u32>,
    /// Conserve version string.
    pub client_version: Option<String>,

    /// Time when the lease was taken.
    #[serde(with = "time::serde::iso8601")]
    pub acquired: OffsetDateTime,

    /// Unix time after which this lease is stale.
    #[serde(with = "time::serde::iso8601")]
    pub expiry: OffsetDateTime,
}

#[cfg(test)]
mod test {
    use std::fs::{write, File};
    use std::process;
    use std::time::Duration;

    use assert_matches::assert_matches;
    use tempfile::TempDir;

    use crate::lease::LEASE_FILENAME;
    use crate::transport::open_local_transport;

    use super::{Lease, LeaseState};

    #[test]
    fn take_lease() {
        let options = super::LeaseOptions {
            lease_expiry: Duration::from_secs(60),
            renewal_interval: Duration::from_secs(10),
        };
        let tmp = TempDir::new().unwrap();
        let transport = open_local_transport(tmp.path()).unwrap();
        let lease = Lease::try_acquire(transport.clone(), &options).unwrap();
        assert!(tmp.path().join("LEASE").exists());
        let orig_lease_taken = lease.content.acquired;

        let peeked = Lease::peek(transport.as_ref()).unwrap();
        let LeaseState::Held(content) = peeked else {
            panic!("lease not held")
        };
        assert_eq!(
            content.host.unwrap(),
            hostname::get().unwrap().to_string_lossy()
        );
        assert_eq!(content.pid, Some(process::id()));

        let lease = lease.renew().unwrap();
        let state2 = Lease::peek(transport.as_ref()).unwrap();
        match state2 {
            LeaseState::Held(content) => {
                assert!(content.acquired > orig_lease_taken);
            }
            _ => panic!("lease should be held, got {state2:?}"),
        }

        lease.release().unwrap();
        assert!(!tmp.path().join("LEASE").exists());
    }

    #[test]
    fn fail_to_renew_deleted_lease() {
        let options = super::LeaseOptions {
            lease_expiry: Duration::from_secs(60),
            renewal_interval: Duration::from_secs(10),
        };
        let tmp = TempDir::new().unwrap();
        let transport = open_local_transport(tmp.path()).unwrap();
        let lease = Lease::try_acquire(transport.clone(), &options).unwrap();
        assert!(tmp.path().join("LEASE").exists());

        transport.remove_file(LEASE_FILENAME).unwrap();

        let result = lease.renew();
        assert_matches!(result, Err(super::Error::Disappeared { .. }));
    }

    #[test]
    fn fail_to_renew_stolen_lease() {
        let options = super::LeaseOptions {
            lease_expiry: Duration::from_secs(60),
            renewal_interval: Duration::from_secs(10),
        };
        let tmp = TempDir::new().unwrap();
        let transport = open_local_transport(tmp.path()).unwrap();
        let lease1 = Lease::try_acquire(transport.clone(), &options).unwrap();
        assert!(tmp.path().join("LEASE").exists());

        // Delete the lease to make it easy to steal.
        transport.remove_file(LEASE_FILENAME).unwrap();
        let lease2 = Lease::try_acquire(transport.clone(), &options).unwrap();
        assert!(tmp.path().join("LEASE").exists());

        // Renewal through the first handle should now fail.
        let result = lease1.renew();
        assert_matches!(result, Err(super::Error::Stolen { .. }));

        // Lease 2 can still renew.
        lease2.renew().unwrap();
    }

    #[test]
    fn peek_fixed_lease_content() {
        let tmp = TempDir::new().unwrap();
        let transport = open_local_transport(tmp.path()).unwrap();
        write(
            tmp.path().join("LEASE"),
            r#"
        {
            "host": "somehost",
            "pid": 1234,
            "client_version": "0.1.2",
            "acquired": "2021-01-01T12:34:56Z",
            "expiry": "2021-01-01T12:35:56Z"
        }"#,
        )
        .unwrap();
        let state = Lease::peek(transport.as_ref()).unwrap();
        dbg!(&state);
        match state {
            LeaseState::Held(content) => {
                assert_eq!(content.host.unwrap(), "somehost");
                assert_eq!(content.pid, Some(1234));
                assert_eq!(content.client_version.unwrap(), "0.1.2");
                assert_eq!(content.acquired.year(), 2021);
                assert_eq!(content.expiry.year(), 2021);
                assert_eq!(
                    content.expiry - content.acquired,
                    time::Duration::seconds(60)
                );
            }
            _ => panic!("lease should be recognized as held, got {state:?}"),
        }
    }

    /// An empty lease file is judged by its mtime; the lease can be grabbed a while
    /// after it was last written.
    #[test]
    fn peek_corrupt_empty_lease() {
        let tmp = TempDir::new().unwrap();
        let transport = open_local_transport(tmp.path()).unwrap();
        File::create(tmp.path().join("LEASE")).unwrap();
        let state = Lease::peek(transport.as_ref()).unwrap();
        match state {
            LeaseState::Corrupt(mtime) => {
                let now = time::OffsetDateTime::now_utc();
                assert!(now - mtime < time::Duration::seconds(15));
            }
            _ => panic!("lease should be recognized as corrupt, got {state:?}"),
        }
    }
}
