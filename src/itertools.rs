#[cfg(test)]

/// Convert a vector of results, into a single overall result
/// whose `Ok` case is a simple `Vec` of values.
///
/// If there are any errors in the iterator, return the first of them.
// TODO: Test this
#[cfg(test)]
pub fn result_iter_to_vec<T, E>(it: &mut Iterator<Item=Result<T, E>>) -> Result<Vec<T>, E> {
    it.collect()
}
