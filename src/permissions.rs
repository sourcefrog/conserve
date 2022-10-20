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

//! Discretionary Access Control permissions for archived files.
//!
//! On unix systems, the mode has 9 significant bits, divided into three classes,
//! owner class, group class, and others class. The owner class permissions
//! apply only to the owner, the group class permissions apply to members of the user's group,
//! and the others class permissions apply to all other users.
//!
//! For each class, there are 3 bitflags: read, write, and execute. This is typically
//! written as an octal number, such as 0o664, which means the user and group can
//! both read and write, and other users can only read.
//!
//! The mode is also often presented as a string of characters, such as "rw-rw-r--",
//! where each character represents one bit.
//!
//! On windows systems, files can be either read-only or writeable. For cross-compatibility,
//! the mode is always stored using the unix format, where the read-only state is stored
//! using the write bit in the user class.
//!
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Permissions {
    mode: u32,
}
impl Default for Permissions {
    fn default() -> Self {
        Self { mode: 0o664 }
    }
}
#[cfg(not(unix))]
impl From<std::fs::Permissions> for Permissions {
    fn from(p: std::fs::Permissions) -> Self {
        Self {
            // set the user class write bit based on readonly status
            // the rest of the bits are left in the default state
            mode: match p.readonly() {
                true => 0o444,
                false => 0o664,
            },
        }
    }
}
#[cfg(windows)]
impl Into<std::fs::Permissions> for Permissions {
    fn into(self) -> std::fs::Permissions {
        // TODO: Actually implement the windows compatibility
        // basically we just need to extract the readonly bit from the mode,
        // but I can't figure out how to instantiate
        std::fs::Permissions::from(std::sys::windows::fs_imp::FilePermissions::new(self.readonly))
    }
}
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
impl From<std::fs::Permissions> for Permissions {
    fn from(p: std::fs::Permissions) -> Self {
        Self { mode: p.mode() }
    }
}
#[cfg(unix)]
impl From<Permissions> for std::fs::Permissions {
    fn from(p: Permissions) -> Self {
        std::fs::Permissions::from_mode(p.mode)
    }
}
