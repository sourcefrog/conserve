// Conserve backup system.
// Copyright 2017 Martin Pool.

/// Abstract Tree trait.

use super::*;

/// Abstract Tree that may be either on the real filesystem or stored in an archive.
pub trait Tree {
    type E: Entry;
    type I: Iterator<Item = Result<Self::E>>;

    // TODO: Maybe hold the report inside self?
    fn iter_entries(&self, report: &Report, excludes: &GlobSet) -> Result<Self::I>;
}
