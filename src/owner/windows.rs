// Conserve backup system.
// Copyright 2022 Stephanie Aelmore.
// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Windows null implementation of file ownership.

use std::fs::Metadata;
use std::io;
use std::path::Path;

use super::Owner;

impl From<&Metadata> for Owner {
    fn from(_: &Metadata) -> Self {
        // TODO: Implement Windows user/group functionality
        Self {
            user: None,
            group: None,
        }
    }
}

pub fn set_owner(_owner: &Owner, _path: &Path) -> io::Result<()> {
    Ok(())
}
