// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! An entry representing a file, directory, etc, in either a
//! stored tree or local tree.

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
}
