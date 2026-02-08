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

//! A log of operations on a transport.
//!
//! This is used so that tests can observe which operations are performed on a transport,
//! to make reproducible assertions that should correlate to runtime performance.
//!
//! This module is only available in test builds.

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Call {
    /// What operation?
    pub verb: Verb,
    /// Path relative to the originally opened transport.
    pub path: String,
}

impl Call {
    pub(crate) fn new<P: ToString>(verb: Verb, path: P) -> Self {
        Self {
            verb,
            path: path.to_string(),
        }
    }
}

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

/// A log of operations on a transport.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Recording {
    /// The operations performed on the transport.
    pub calls: Vec<Call>,
}

impl Recording {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(super) fn push(&mut self, call: Call) {
        self.calls.push(call);
    }

    #[cfg(test)]
    pub fn verb_paths(&self, verb: Verb) -> Vec<&str> {
        self.calls
            .iter()
            .filter(|call| call.verb == verb)
            .map(|call| call.path.as_str())
            .collect()
    }

    #[cfg(test)]
    /// Assert that there were a certain number of calls with the given verb.
    pub fn assert_verb_count(&self, verb: Verb, expected_count: usize, message: &str) {
        let calls = self
            .calls
            .iter()
            .filter(|call| call.verb == verb)
            .collect::<Vec<_>>();
        let count = calls.len();
        assert_eq!(
            count, expected_count,
            "Expected {expected_count} calls with verb {verb:?}, but found {count}: {message}: {calls:#?}",
        );
    }
}
