// Conserve backup system.
// Copyright 2015 Martin Pool.

//! Bands are the top-level structure inside an archive.
//!
//! Each band contains up to one version of each file, arranged in sorted order within the
//! band.
//!
//! Bands can stack on top of each other to create a tree of incremental backups.

/// Bands have an id which is a sequence of one or more non-negative integers. This is externally
/// represented as a string like `b0001-0010`, which becomes their directory name in the archive.
///
/// ```
/// use conserve::band::BandId;
/// let band_id = BandId::new(&[1, 10, 20]);
/// assert_eq!(band_id.as_string(), "b0001-0010-0020");
/// ```
///
/// Numbers are zero-padded to what should normally be a reasonable length, but they can
/// overflow:
///
/// ```
/// assert_eq!(conserve::band::BandId::new(&[1000000, 2000000]).as_string(),
///            "b1000000-2000000")
/// ```

#[derive(Debug, PartialEq)]
pub struct BandId {
    /// The sequence numbers at each tier.
    seqs: Vec<u32>,
    
    /// The pre-calculated string form for this id.
    string_form: String,
}


impl BandId {
    /// Makes a new BandId from a sequence of integers.
    pub fn new(seqs: &[u32]) -> BandId {
        assert!(seqs.len() > 0);
        BandId{
            seqs: seqs.to_vec(),
            string_form: BandId::make_string_form(seqs),
        }
    }

    /// Make a new BandId from a string form.
    ///
    /// ```
    /// use conserve::band::BandId;
    /// let band = BandId::from_string("b0001-1234").unwrap();
    /// assert_eq!(band.as_string(), "b0001-1234");
    /// ```
    pub fn from_string(s: &str) -> Option<BandId> {
        if s.chars().next() != Some('b') {
            return None
        }
        let mut seqs = Vec::<u32>::new();
        for num_part in s[1..].split('-') {
            match num_part.parse::<u32>() {
                Ok(num) => seqs.push(num),
                Err(..) => return None,
            }
        }
        if seqs.is_empty() {
            None
        } else {
            // This rebuilds a new string form to get it into the canonical form.
            Some(BandId::new(&seqs))
        }
    }
    
    /// Returns the string representation of this BandId.
    pub fn as_string(self: &BandId) -> &String {
        &self.string_form
    }
    
    fn make_string_form(seqs: &[u32]) -> String {
        let mut result = String::with_capacity(30);
        result.push_str("b");
        for s in seqs {
            result.push_str(&format!("{:04}-", s));
        }
        result.pop(); // remove the last dash
        result.shrink_to_fit();
        result
    }
    
    // TODO: Maybe a more concise debug form?
}

 
#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::*;

    #[test]
    #[should_panic]
    fn test_empty_id_not_allowed() {
        BandId::new(&[]);
    }

    #[test]
    fn test_from_string_detects_invalid() {
        assert_eq!(BandId::from_string(""), None);
        assert_eq!(BandId::from_string("hello"), None);
        assert_eq!(BandId::from_string("b"), None);
        assert_eq!(BandId::from_string("b-"), None);
        assert_eq!(BandId::from_string("b2-"), None);
        assert_eq!(BandId::from_string("b-2"), None);
        assert_eq!(BandId::from_string("b2-1-"), None);
        assert_eq!(BandId::from_string("b2--1"), None);
        assert_eq!(BandId::from_string("beta"), None);
        assert_eq!(BandId::from_string("b-eta"), None);
        assert_eq!(BandId::from_string("b-1eta"), None);
        assert_eq!(BandId::from_string("b-1-eta"), None);
    }

    #[test]
    fn test_from_string_valid() {
        assert_eq!(BandId::from_string("b0001").unwrap().as_string(), "b0001");
        assert_eq!(BandId::from_string("b123456").unwrap().as_string(), "b123456");
        assert_eq!(BandId::from_string("b0001-0100-0234").unwrap().as_string(),
            "b0001-0100-0234");
    }
}
