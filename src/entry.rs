// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! An entry representing a file, directory, etc, in either a
//! stored tree or local tree.

use super::*;


/// Kind of file that can be stored in the archive.
#[derive(Clone, Copy, Debug, PartialEq, RustcDecodable, RustcEncodable)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    /// Unknown file observed in local tree. Shouldn't be stored.
    Unknown,
}


/// A file, directory, or symlink stored in any tree.
pub trait Entry {
    fn kind(&self) -> Kind;

    // TODO: Would be better to return a reference, but it's difficult because IndexEntry doesn't
    // directly store an Apath due to serialization.
    fn apath(&self) -> Apath;
}
