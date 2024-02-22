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
use std::fs::FileType;

use serde::{Deserialize, Serialize};

/// Kind of file that can be stored in the archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    /// Unknown file observed in local tree. Shouldn't be stored.
    Unknown,
}

impl From<FileType> for Kind {
    fn from(ft: FileType) -> Kind {
        if ft.is_file() {
            Kind::File
        } else if ft.is_dir() {
            Kind::Dir
        } else if ft.is_symlink() {
            Kind::Symlink
        } else {
            Kind::Unknown
        }
    }
}
