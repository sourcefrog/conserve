// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! An entry representing a file, directory, etc, in either a 
//! stored tree or local tree.

/// Kind of file that can be stored in the archive.
#[derive(Debug, PartialEq, RustcDecodable, RustcEncodable)]
pub enum Kind {
    File,
    Dir,
    Symlink,
}



