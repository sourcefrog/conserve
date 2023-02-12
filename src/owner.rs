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

//! Stores the user and group as Strings in the archive.
//! There is potentially a more efficient way to do this, but this approach works
//! better than just saving the uid and gid, so that backups may potentially
//! be restored on a different system.

use std::fmt::Display;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix::set_owner;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::set_owner;

#[derive(Default, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Owner {
    /// TODO: Maybe the strings can be 'static references to the cache?
    pub user: Option<String>,
    pub group: Option<String>,
}

impl Owner {
    pub fn is_none(&self) -> bool {
        self.user.is_none() && self.group.is_none()
    }

    pub fn set_owner<P: AsRef<Path>>(&self, path: P) -> crate::Result<()> {
        set_owner(self, path.as_ref())
    }
}

impl Display for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let none = "none".to_string();
        write!(
            f,
            "{:<10} {:<10}",
            if let Some(user) = &self.user {
                user
            } else {
                &none
            },
            if let Some(group) = &self.group {
                group
            } else {
                &none
            }
        )
    }
}
