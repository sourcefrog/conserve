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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UnixMode {
    pub mode: u32,
}
impl Default for UnixMode {
    fn default() -> Self {
        // created with execute permission so that restoring old archives works properly
        // TODO: ideally we would set this based on the inode type read from the archive
        Self { mode: 0o100775 }
    }
}
impl UnixMode {
    pub fn readonly(self) -> bool {
        // determine if a file is readonly based on whether the owner can write it
        self.mode & 0o000200 == 0
    }
}
impl fmt::Display for UnixMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sss = (self.mode & 0o7000) >> 9;
        let owner = (self.mode & 0o0700) >> 6;
        let group = (self.mode & 0o0070) >> 3;
        let other = self.mode & 0o0007;

        // owner permissions
        write!(f, "{}", if owner & 0b100 > 0 { 'r' } else { '-' })?;
        write!(f, "{}", if owner & 0b010 > 0 { 'w' } else { '-' })?;
        if sss == 0b100 {
            // Set UID
            write!(f, "{}", if owner & 0b001 > 0 { 's' } else { 'S' })?;
        } else {
            write!(f, "{}", if owner & 0b001 > 0 { 'x' } else { '-' })?;
        }

        // group permissions
        write!(f, "{}", if group & 0b100 > 0 { 'r' } else { '-' })?;
        write!(f, "{}", if group & 0b010 > 0 { 'w' } else { '-' })?;
        if sss == 0b010 {
            // Set GID
            write!(f, "{}", if group & 0b001 > 0 { 's' } else { 'S' })?;
        } else {
            write!(f, "{}", if group & 0b001 > 0 { 'x' } else { '-' })?;
        }

        // other permissions
        write!(f, "{}", if other & 0b100 > 0 { 'r' } else { '-' })?;
        write!(f, "{}", if other & 0b010 > 0 { 'w' } else { '-' })?;
        if sss == 0b001 {
            // sticky
            write!(f, "{}", if other & 0b001 > 0 { 't' } else { 'T' })?;
        } else {
            write!(f, "{}", if other & 0b001 > 0 { 'x' } else { '-' })?;
        }

        Ok(())
    }
}
impl From<u32> for UnixMode {
    fn from(mode: u32) -> Self {
        Self { mode }
    }
}
impl From<&str> for UnixMode {
    // TODO: implement set uid, set gid, and sticky
    fn from(s: &str) -> Self {
        let mut mode: u32 = 0;
        for (n, c) in s.chars().enumerate() {
            if n % 3 == 0 {
                mode <<= 3;
            }
            mode += match c {
                'r' => 0b100,
                'w' => 0b010,
                'x' => 0b001,
                _ => 0,
            };
        }

        Self { mode }
    }
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
impl From<Permissions> for UnixMode {
    fn from(p: Permissions) -> Self {
        Self { mode: p.mode() }
    }
}
#[cfg(unix)]
impl From<UnixMode> for Permissions {
    fn from(p: UnixMode) -> Self {
        Permissions::from_mode(p.mode)
    }
}
#[cfg(not(unix))]
impl From<Permissions> for UnixMode {
    fn from(p: Permissions) -> Self {
        Self {
            // set the user class write bit based on readonly status
            // the rest of the bits are left in the default state
            // TODO: fix this and test on windows
            mode: match p.readonly() {
                true => 0o100555,
                false => 0o100775,
            },
        }
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
    #[test]
    fn from_str() {
        assert_eq!(UnixMode::from("rwxrwxr--"), UnixMode { mode: 0o774 });
        assert_eq!(UnixMode::from("rwxr-xr-x"), UnixMode { mode: 0o755 });
        assert_eq!(UnixMode::from("rwxr---wx"), UnixMode { mode: 0o743 });
        assert_eq!(UnixMode::from("---r---wx"), UnixMode { mode: 0o043 });
    }
}
