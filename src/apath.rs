// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

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
//! There is a total ordering of apaths such that all the direct children of an archive sort
//! before its subdirectories, and the contents of a directory are sorted in UTF-8 order.
//!
//! Apaths in memory are simply strings.

// use std::cmp::Ordering;


/// Compare two apaths.
// pub fn apath_cmp(a: &str, b: &str) -> Ordering {
//     panic!("unimplemented");
// }


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
    // use std::cmp::Ordering;
    use super::{apath_valid};

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
        // assert_eq!(apath_cmp("a", "a"), Ordering::Equal);
    }
}
