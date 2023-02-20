// Conserve backup system.
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

//! An entry representing a file, directory, etc, in either a
//! stored tree or local tree.

use std::fmt::Debug;

use time::OffsetDateTime;

use crate::kind::Kind;
use crate::owner::Owner;
use crate::unix_mode::UnixMode;
use crate::*;

pub trait Entry: Debug {
    fn apath(&self) -> &Apath;
    fn kind(&self) -> Kind;
    fn mtime(&self) -> OffsetDateTime;
    fn size(&self) -> Option<u64>;
    fn symlink_target(&self) -> &Option<String>;
    fn unix_mode(&self) -> UnixMode;
    fn owner(&self) -> Owner;
}
