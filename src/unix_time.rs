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
//! In particular, glue between [filetime] and [time].

use filetime::FileTime;
use time::OffsetDateTime;

pub(crate) trait FromUnixAndNanos {
    fn from_unix_seconds_and_nanos(unix_seconds: i64, nanoseconds: u32) -> Self;
}

impl FromUnixAndNanos for OffsetDateTime {
    fn from_unix_seconds_and_nanos(unix_seconds: i64, nanoseconds: u32) -> Self {
        OffsetDateTime::from_unix_timestamp(unix_seconds)
            .unwrap()
            .replace_nanosecond(nanoseconds)
            .unwrap()
    }
}

pub(crate) trait ToOffsetDateTime {
    fn to_offset_date_time(&self) -> OffsetDateTime;
}

impl ToOffsetDateTime for FileTime {
    fn to_offset_date_time(&self) -> OffsetDateTime {
        OffsetDateTime::from_unix_seconds_and_nanos(self.unix_seconds(), self.nanoseconds())
    }
}

pub(crate) trait ToFileTime {
    fn to_file_time(&self) -> FileTime;
}

impl ToFileTime for OffsetDateTime {
    fn to_file_time(&self) -> FileTime {
        FileTime::from_unix_time(self.unix_timestamp(), self.nanosecond())
    }
}
