// Conserve backup system.
// Copyright 2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Strategies for damaging files.

use std::fs::OpenOptions;
use std::path::Path;

/// A way of damaging a file in an archive.
#[derive(Debug, Clone)]
pub enum Damage {
    /// Truncate the file to zero bytes.
    Truncate,
}

impl Damage {
    /// Apply this damage to a file.
    ///
    /// The file must already exist.
    pub fn damage(&self, path: &Path) {
        match self {
            Damage::Truncate => {
                assert!(path.exists(), "{path:?} does not exist");
                OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(path)
                    .expect("truncate file");
            }
        }
    }
}
