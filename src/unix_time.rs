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

/// Helper to construct a Timestamp from Unix seconds and nanoseconds.
pub(crate) fn timestamp_from_unix_nanos(unix_seconds: i64, nanoseconds: u32) -> Timestamp {
    Timestamp::from_second(unix_seconds)
        .unwrap()
        .checked_add(jiff::Span::new().nanoseconds(nanoseconds as i64))
        .unwrap()
}

/// Helper to convert a Timestamp to a FileTime.
pub(crate) fn timestamp_to_file_time(timestamp: &Timestamp) -> FileTime {
    FileTime::from_unix_time(timestamp.as_second(), timestamp.subsec_nanosecond() as u32)
}
