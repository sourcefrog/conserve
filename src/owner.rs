// Copyright 2022 Stephanie Aelmore.
// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020, 2021, 2022 Martin Pool.

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

use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{fmt::Display, fs::Metadata};

// TODO: maybe set the default to the current user and group?
// do we want to do that in our default impl here?
#[derive(Default, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Owner {
    pub user: Option<String>,
    pub group: Option<String>,
}
impl Owner {
    pub fn is_none(&self) -> bool {
        self.user.is_none() && self.group.is_none()
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

impl From<&Metadata> for Owner {
    #[cfg(unix)]
    fn from(mdata: &Metadata) -> Self {
        let user = if let Some(user) = users::get_user_by_uid(mdata.uid()) {
            user.name().to_str().map(|name| name.to_string())
        } else {
            None
        };
        let group = if let Some(group) = users::get_group_by_gid(mdata.gid()) {
            group.name().to_str().map(|name| name.to_string())
        } else {
            None
        };

        Self { user, group }
    }
    #[cfg(not(unix))]
    fn from(_: &Metadata) -> Self {
        // TODO: Implement Windows user/group functionality
        Self {
            user: None,
            group: None,
        }
    }
}
