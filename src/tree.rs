// Conserve backup system.
// Copyright 2017-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Abstract Tree trait.

use std::sync::Arc;

use crate::monitor::Monitor;
use crate::*;

/// Abstract Tree that may be either on the real filesystem or stored in an archive.
// TODO: Maybe delete this: abstracting over stored and live trees is not very useful
// and the patterns to get the best performance are different.
pub trait ReadTree {
    type Entry: EntryTrait + 'static;
    type IT: Iterator<Item = Self::Entry>;

    /// Iterate, in apath order, all the entries in this tree.
    ///
    /// Errors reading individual paths or directories are sent to the UI and
    /// counted, but are not treated as fatal, and don't appear as Results in the
    /// iterator.
    fn iter_entries(
        &self,
        subtree: Apath,
        exclude: Exclude,
        monitor: Arc<dyn Monitor>,
    ) -> Result<Self::IT>;
}

/// The measured size of a tree.
pub struct TreeSize {
    pub file_bytes: u64,
}
