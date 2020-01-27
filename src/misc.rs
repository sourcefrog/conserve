// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Generally useful functions.

/// Remove and return an item from a vec, if it's present.
pub(crate) fn remove_item<T, U: PartialEq<T>>(v: &mut Vec<T>, item: &U) {
    if let Some(pos) = v.iter().position(|x| *item == *x) {
        v.remove(pos);
    }
}

pub(crate) fn bytes_to_human_mb(s: u64) -> String {
    use thousands::Separable;
    (s / 1_000_000).separate_with_commas()
}
