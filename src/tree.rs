// Conserve backup system.
// Copyright 2017 Martin Pool.

//! Abstract Tree trait.

use super::*;

/// Abstract Tree that may be either on the real filesystem or stored in an archive.
pub trait Tree {
    type E: Entry;
    type I: Iterator<Item = Result<Self::E>>;
    type R: std::io::Read;

    fn iter_entries(&self, excludes: &GlobSet) -> Result<Self::I>;
    fn file_contents(&self, entry: &Self::E) -> Result<Self::R>;
}


/// A tree open for writing, either local or an an archive.
///
/// This isn't a sub-trait of Tree since a backup band can't be read while writing is
/// still underway.
///
/// Entries must be written in Apath order, since that's a requirement of the index.
pub trait WriteTree {
    fn finish(&mut self) -> Result<()>;

    fn write_dir(&mut self, entry: &Entry) -> Result<()>;
    fn write_symlink(&mut self, entry: &Entry) -> Result<()>;
    fn write_file(&mut self, entry: &Entry, content: &mut std::io::Read) -> Result<()>;
}
