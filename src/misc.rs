// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

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
