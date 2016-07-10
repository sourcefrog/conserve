#[cfg(test)]
use std::io;


/// Convert a vector of results, into a single overall result
/// whose `Ok` case is a simple `Vec` of values.
///
/// If there are any errors in the iterator, return the first of them.
// TODO: Test this
#[cfg(test)]
pub fn result_iter_to_vec<T>(it: &mut Iterator<Item=io::Result<T>>) -> io::Result<Vec<T>> {
    let mut result = Vec::<T>::new();
    for i in it {
        match i {
            Ok(val) => { result.push(val) },
            Err(e) => { return Err(e) },
        }
    }
    Ok(result)
}
