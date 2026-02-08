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
//!
//! In particular, glue between [filetime] and [jiff].

use filetime::FileTime;
use jiff::Timestamp;
use std::time::SystemTime;

pub(crate) trait FromUnixAndNanos {
    fn from_unix_seconds_and_nanos(unix_seconds: i64, nanoseconds: u32) -> Self;
}

impl FromUnixAndNanos for Timestamp {
    fn from_unix_seconds_and_nanos(unix_seconds: i64, nanoseconds: u32) -> Self {
        Timestamp::from_second(unix_seconds)
            .unwrap()
            .checked_add(jiff::Span::new().nanoseconds(nanoseconds as i64))
            .unwrap()
    }
}

#[allow(unused)] // really unused at present, but might be useful
pub trait ToTimestamp {
    fn to_timestamp(&self) -> Timestamp;
}

impl ToTimestamp for FileTime {
    fn to_timestamp(&self) -> Timestamp {
        Timestamp::from_unix_seconds_and_nanos(self.unix_seconds(), self.nanoseconds())
    }
}

impl ToTimestamp for SystemTime {
    fn to_timestamp(&self) -> Timestamp {
        match self.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(dur) => {
                let secs = dur.as_secs() as i64;
                let nanos = dur.subsec_nanos();
                Timestamp::from_unix_seconds_and_nanos(secs, nanos)
            }
            Err(e) => {
                // Time is before Unix epoch
                let dur = e.duration();
                let secs = -(dur.as_secs() as i64);
                let nanos = dur.subsec_nanos();
                if nanos == 0 {
                    Timestamp::from_unix_seconds_and_nanos(secs, 0)
                } else {
                    // Need to adjust for the fractional part
                    Timestamp::from_unix_seconds_and_nanos(secs - 1, 1_000_000_000 - nanos)
                }
            }
        }
    }
}

pub(crate) trait ToFileTime {
    fn to_file_time(&self) -> FileTime;
}

impl ToFileTime for Timestamp {
    fn to_file_time(&self) -> FileTime {
        FileTime::from_unix_time(self.as_second(), self.subsec_nanosecond() as u32)
    }
}
