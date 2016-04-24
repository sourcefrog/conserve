// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

#![allow(dead_code)]  // Until linked in

//! "Apaths" (for archive paths) are platform-independent relative file paths used inside archive
//! snapshots.
//!
//! Archive paths are:
//!
//!  * Case-sensitive.
//!  * Components are separated by `/`.
//!  * UTF-8, without consideration of normalization.
//!  * Do not contain `.`, `..`, or empty components.
//!  * Implicitly relative to the base of the backup directory.
//!
//! There is a total ordering of apaths such that all the direct children of an directory sort
//! before its subdirectories, and the contents of a directory are sorted in UTF-8 order.
//!
//! Apaths in memory are simply strings.

use std::cmp::Ordering;


/// Compare two apaths.
pub fn apath_cmp(a: &str, b: &str) -> Ordering {
    let mut ait = a.split('/').peekable();
    let mut bit = b.split('/').peekable();
    loop {
        match (ait.next(), bit.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_bc)) => return Ordering::Less,
            (Some(_ac), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) =>
                // If one is a direct child and the other is in a subdirectory,
                // the direct child comes first.
                match (ait.peek().is_none(), bit.peek().is_none()) {
                    (true, true) => return ac.cmp(bc),
                    (true, false) => return Ordering::Less,
                    (false, true) => return Ordering::Greater,
                    (false, false) => match ac.cmp(bc) {
                        Ordering::Equal => continue,
                        o => return o,
                    }
                }
        }
    }
}


/// True if this apath is well-formed.
///
/// Rust strings are by contract always valid UTF-8, so to meet that requirement for apaths it's
/// enough to use a checked conversion from bytes or an OSString.
pub fn apath_valid(a: &str) -> bool {
    for part in a.split('/') {
        if part.is_empty() {
            // Repeated slash or slash at start of string.
            return false;
        } else if part == "." || part == ".." {
            return false;
        } else if part.contains('\0') {
            return false;
        }
    }
    true
}


#[cfg(test)]
mod tests {
    use super::{apath_cmp, apath_valid};

    #[test]
    pub fn test_apath_valid() {
        let valid_cases = [
            "a",
            "a/b",
            "a/b/c",
            "a/.config",
            "a/..obscure",
            "a/...",
            "kleine Katze Fuß",
        ];
        for v in valid_cases.into_iter() {
            if !apath_valid(&v) {
                panic!("{:?} incorrectly marked invalid", v);
            }
        }
    }

    #[test]
    pub fn test_apath_invalid() {
        let invalid_cases = [
            "/",
            "/a",
            "a//b",
            "a/",
            "a//",
            "./a/b",
            "a/b/.",
            "a/./b",
            "a/b/../c",
            "../a",
            "hello\0",
        ];
        for v in invalid_cases.into_iter() {
            if apath_valid(&v) {
                panic!("{:?} incorrectly marked valid", v);
            }
        }
    }

    #[test]
    pub fn test_apath_cmp() {
        let ordered = [
            "...a",
            ".a",
            "a",
            "b",
            "~~",
            "ñ",
            "a/1",
            "a/100",
            "a/2",
            "a/añejo",
            "b/((",
            "b/,",
            "b/A",
            "b/AAAA",
            "b/a",
            "b/b",
            "b/c",
            "b/a/c",
            "b/b/c",
            "b/b/b/z",
            "b/b/b/{zz}",
        ];
        for i in 0..ordered.len() {
            let a = ordered[i];
            for j in 0..ordered.len() {
                let b = ordered[j];
                let expected_order = i.cmp(&j);
                let r = apath_cmp(a, b);
                if r != expected_order {
                    panic!("apath_cmp({:?}, {:?}): returned {:?} expected {:?}",
                        a, b, r, expected_order);
                }
            }
        };
    }
}
