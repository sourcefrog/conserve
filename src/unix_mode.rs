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
//! TODO: Properly implement and test Windows compatibility.
//!
use serde::{Deserialize, Serialize};
use std::{fmt, fs::Permissions};
use unix_mode;

#[derive(Debug, Clone, Copy, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UnixMode(u32);

// bit mask for the bits in the unix mode that this struct will store.
// masks all bits other than the permissions, sticky, and set bits
const MODE_BITS: u32 = 0o7777;

// TODO: do we want to set permissions based on inode type?
impl Default for UnixMode {
    fn default() -> Self {
        // created with execute permission so that restoring old archives works properly
        // (searching directories requires them to have exec permission)
        Self(0o775)
    }
}
impl PartialEq for UnixMode {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
// Assert that equivalence is reflexive
impl Eq for UnixMode {}

impl UnixMode {
    pub fn readonly(self) -> bool {
        // determine if a file is readonly based on whether the owning user can write to it
        self.0 & 0o200 == 0
    }
}
impl fmt::Display for UnixMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Convert to string. Since the file type bits are stripped, there will
        // be a leading question mark from unix_mode::to_string, which we will strip.
        write!(
            f,
            "{}",
            unix_mode::to_string(self.0).trim_start_matches('?')
        )
    }
}
impl From<u32> for UnixMode {
    fn from(mode: u32) -> Self {
        Self(mode & MODE_BITS)
    }
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
impl From<Permissions> for UnixMode {
    fn from(p: Permissions) -> Self {
        Self(p.mode() & MODE_BITS)
    }
}
#[cfg(unix)]
impl From<UnixMode> for Permissions {
    fn from(u: UnixMode) -> Self {
        Permissions::from_mode(u.0)
    }
}
#[cfg(not(unix))]
impl From<Permissions> for UnixMode {
    fn from(p: Permissions) -> Self {
        Self(
            // set the user class write bit based on readonly status
            // the rest of the bits are left in the default state
            // TODO: fix this and test on windows
            match p.readonly() {
                true => 0o555,
                false => 0o775,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::unix_mode::UnixMode;
    #[test]
    fn display_unix_modes() {
        assert_eq!("rwxrwxr--", format!("{}", UnixMode::from(0o774)));
        assert_eq!("rwxr-xr-x", format!("{}", UnixMode::from(0o755)));
        assert_eq!("rwxr---wx", format!("{}", UnixMode::from(0o743)));
        assert_eq!("---r---wx", format!("{}", UnixMode::from(0o043)));
        assert_eq!("rwsr-xr-x", format!("{}", UnixMode::from(0o4755)));
        assert_eq!("rwxr-sr-x", format!("{}", UnixMode::from(0o2755)));
        assert_eq!("rwxr-xr-t", format!("{}", UnixMode::from(0o1755)));
        assert_eq!("rwxrwxr-T", format!("{}", UnixMode::from(0o1774)));
        assert_eq!("rwxr-S-wx", format!("{}", UnixMode::from(0o2743)));
        assert_eq!("--Sr---wx", format!("{}", UnixMode::from(0o4043)));
    }
}
