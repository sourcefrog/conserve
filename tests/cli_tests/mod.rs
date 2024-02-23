// Conserve backup system.
// Copyright 2016-2024 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Run conserve CLI as a subprocess and test it.

mod backup;
mod delete;
mod diff;
mod exclude;
pub mod ls;
mod trace;
mod validate;
mod versions;

#[cfg(unix)]
mod unix {
    mod diff;
    mod permissions;
}
