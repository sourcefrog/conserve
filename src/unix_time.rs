// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Unix timestamps.

use std::convert::From;
use std::time::{SystemTime, UNIX_EPOCH};

/// A Unix time, as seconds since 1970 UTC, plus fractional nanoseconds.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct UnixTime {
    /// Whole seconds after (or if negative, before) 1 Jan 1970 UTC.
    pub secs: i64,
    /// Fractional nanoseconds.
    pub nanosecs: u32,
}

impl From<SystemTime> for UnixTime {
    fn from(t: SystemTime) -> UnixTime {
        if let Ok(after) = t.duration_since(UNIX_EPOCH) {
            UnixTime {
                secs: after.as_secs() as i64,
                nanosecs: after.subsec_nanos(),
            }
        } else {
            let before = UNIX_EPOCH.duration_since(t).unwrap();
            let mut secs = -(before.as_secs() as i64);
            let mut nanosecs = before.subsec_nanos();
            if nanosecs > 0 {
                secs -= 1;
                nanosecs = 1_000_000_000 - nanosecs;
            }
            UnixTime { secs, nanosecs }
        }
    }
}
