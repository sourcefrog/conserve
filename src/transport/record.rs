// Copyright 2025 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! A log of operations on a transport, for testing.

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Call(
    /// What operation?
    pub Verb,
    /// Path relative to the originally opened transport.
    pub String,
);

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Verb {
    ListDir,
    Write,
    Read,
    RemoveFile,
    RemoveDirAll,
    CreateDir,
    Metadata,
}
