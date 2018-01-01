// Conserve backup system.
// Copyright 2017 Martin Pool.

/// Abstract Tree trait.

use super::*;

/// Abstract Tree that may be either on the real filesystem or stored in an archive.
pub trait Tree {
    type E: Entry;
    type I: Iterator<Item = Result<Self::E>>;

    fn iter_entries(&self, excludes: &GlobSet) -> Result<Self::I>;
}
