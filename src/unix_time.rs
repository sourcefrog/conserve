// Conserve backup system.
// Copyright 2015-2026 Martin Pool.

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

// TODO: delete this if <https://github.com/alexcrichton/filetime/pull/118/changes>
// is merged and released.

use filetime::FileTime;
use jiff::Timestamp;

pub(crate) trait ToFileTime {
    fn to_file_time(&self) -> FileTime;
}

impl ToFileTime for Timestamp {
    fn to_file_time(&self) -> FileTime {
        FileTime::from_unix_time(self.as_second(), self.subsec_nanosecond().cast_unsigned())
    }
}
