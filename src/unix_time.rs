// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Times relative to the Unix epoch.

use filetime::FileTime;

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

impl From<UnixTime> for FileTime {
    fn from(t: UnixTime) -> FileTime {
        FileTime::from_unix_time(t.secs, t.nanosecs)
    }
}
