// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Generally useful functions.

use std::time::Duration;

use crate::stats::Sizes;

/// Remove and return an item from a vec, if it's present.
pub(crate) fn remove_item<T, U: PartialEq<T>>(v: &mut Vec<T>, item: &U) {
    // Remove this when it's stabilized in std:
    // https://github.com/rust-lang/rust/issues/40062
    if let Some(pos) = v.iter().position(|x| *item == *x) {
        v.remove(pos);
    }
}

pub fn bytes_to_human_mb(s: u64) -> String {
    use thousands::Separable;
    let mut s = (s / 1_000_000).separate_with_commas();
    s.push_str(" MB");
    s
}

/// True if `a` is zero.
///
/// This trivial function exists as a predicate for serde.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn zero_u32(a: &u32) -> bool {
    *a == 0
}

/// True if `a` is zero.
///
/// This trivial function exists as a predicate for serde.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn zero_u64(a: &u64) -> bool {
    *a == 0
}

#[allow(unused)]
pub(crate) fn compression_percent(s: &Sizes) -> i64 {
    if s.uncompressed > 0 {
        100i64 - (100 * s.compressed / s.uncompressed) as i64
    } else {
        0
    }
}

pub fn duration_to_hms(d: Duration) -> String {
    let elapsed_secs = d.as_secs();
    if elapsed_secs >= 3600 {
        format!(
            "{:2}:{:02}:{:02}",
            elapsed_secs / 3600,
            (elapsed_secs / 60) % 60,
            elapsed_secs % 60
        )
    } else {
        format!("   {:2}:{:02}", (elapsed_secs / 60) % 60, elapsed_secs % 60)
    }
}

#[allow(unused)]
pub(crate) fn mbps_rate(bytes: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs() as f64 + f64::from(elapsed.subsec_millis()) / 1000.0;
    if secs > 0.0 {
        bytes as f64 / secs / 1e6
    } else {
        0f64
    }
}

/// Describe the compression ratio: higher is better.
#[allow(unused)]
pub(crate) fn compression_ratio(s: &Sizes) -> f64 {
    if s.compressed > 0 {
        s.uncompressed as f64 / s.compressed as f64
    } else {
        0f64
    }
}

/// Adds `Result::inspect_err` which is not yet stabilized.
pub(crate) trait ResultExt {
    type T;
    type E;
    fn our_inspect_err<F: FnOnce(&Self::E)>(self, f: F) -> Self;
}

impl<T, E> ResultExt for std::result::Result<T, E> {
    type T = T;
    type E = E;

    #[inline]
    fn our_inspect_err<F: FnOnce(&E)>(self, f: F) -> Self {
        if let Err(ref e) = self {
            f(e);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_compression_ratio() {
        let ratio = compression_ratio(&Sizes {
            compressed: 2000,
            uncompressed: 4000,
        });
        assert_eq!(format!("{ratio:3.1}x"), "2.0x");
    }
}
