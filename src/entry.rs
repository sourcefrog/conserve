// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

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

use crate::kind::Kind;
use crate::owner::Owner;
use crate::unix_mode::UnixMode;
use crate::unix_time::UnixTime;
use crate::*;

pub trait Entry: Debug + Eq + PartialEq {
    fn apath(&self) -> &Apath;
    fn kind(&self) -> Kind;
    fn mtime(&self) -> UnixTime;
    fn size(&self) -> Option<u64>;
    fn symlink_target(&self) -> &Option<String>;
    fn unix_mode(&self) -> UnixMode;
    fn owner(&self) -> Owner;

    /// True if the metadata supports an assumption the file contents have
    /// not changed.
    fn is_unchanged_from<O: Entry>(&self, basis_entry: &O) -> bool {
        basis_entry.kind() == self.kind()
            && basis_entry.mtime() == self.mtime()
            && basis_entry.size() == self.size()
            && basis_entry.unix_mode() == self.unix_mode()
            && basis_entry.owner() == self.owner()
    }
}
