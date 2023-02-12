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

//! Unix implementation of file ownership.

use std::io;
use std::os::unix::fs::MetadataExt;
use std::sync::Mutex;
use std::{fs, path::Path};

use lazy_static::lazy_static;
use nix::errno::Errno;
use nix::unistd;
use users::{Groups, Users, UsersCache};

use super::Owner;
use crate::{Error, Result};

lazy_static! {
    static ref USERS_CACHE: Mutex<UsersCache> = Mutex::new(UsersCache::new());
}

impl From<&fs::Metadata> for Owner {
    fn from(mdata: &fs::Metadata) -> Self {
        let users_cache = USERS_CACHE.lock().unwrap();
        let user: Option<String> = users_cache
            .get_user_by_uid(mdata.uid())
            .and_then(|user| user.name().to_str().map(String::from));
        let group: Option<String> = users_cache
            .get_group_by_gid(mdata.gid())
            .and_then(|group| group.name().to_str().map(String::from));
        Self { user, group }
    }
}

pub(crate) fn set_owner(owner: &Owner, path: &Path) -> Result<()> {
    let users_cache = USERS_CACHE.lock().unwrap();
    let uid_opt = owner
        .user
        .as_ref()
        .and_then(|user| users_cache.get_user_by_name(&user))
        .map(|user| user.uid())
        .map(unistd::Uid::from_raw);
    let gid_opt = owner
        .group
        .as_ref()
        .and_then(|group| users_cache.get_group_by_name(&group))
        .map(|group| group.gid())
        .map(unistd::Gid::from_raw);
    drop(users_cache);
    // TODO: use `std::os::unix::fs::chown(path, uid, gid)?;` once stable
    match unistd::chown(path, uid_opt, gid_opt) {
        Ok(()) => Ok(()),
        Err(errno) if errno == Errno::EPERM => {
            // If the restore is not run as root (or with special capabilities)
            // then we probably can't set ownership, and there's no point
            // complaining
            Ok(())
        }
        Err(errno) => Err(Error::SetOwner {
            path: path.to_path_buf(),
            source: io::Error::from_raw_os_error(errno as i32),
        }),
    }
}
