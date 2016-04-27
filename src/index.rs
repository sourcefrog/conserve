// Conserve backup system.
// Copyright 2015 Martin Pool.

///! Listing of files in a band in the archive.

// use rustc_serialize::json;

/// Kind of file that can be stored in the archive.
#[derive(Debug, RustcDecodable, RustcEncodable)]
enum IndexKind {
    File,
    Dir,
    Symlink,
}

/// Description of one archived file.
#[derive(Debug, RustcDecodable, RustcEncodable)]
struct IndexEntry {
    /// Path of this entry relative to the base of the backup, in `apath` form.
    pub apath: String,

    /// File modification time, in whole seconds past the Unix epoch.
    pub mtime: u64,

    /// Type of file.
    pub kind: IndexKind,

    /// BLAKE2b hash of the entire original file.
    pub blake2b: String,
}

#[cfg(test)]
mod tests {
    use super::{IndexEntry, IndexKind};
    use rustc_serialize::json;

    const EXAMPLE_HASH: &'static str =
        "66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf21\
         45b117b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c";

    #[test]
    fn test_serialize_index() {
        let entries = [IndexEntry {
            apath: "a/b".to_string(),
            mtime: 1461736377,
            kind: IndexKind::File,
            blake2b: EXAMPLE_HASH.to_string(),
        }];
        let index_json = json::encode(&entries).unwrap();
        println!("{}", index_json);
        assert_eq!(
            index_json,
            "[{\"apath\":\"a/b\",\
            \"mtime\":1461736377,\
            \"kind\":\"File\",\
            \"blake2b\":\"66ad1939a9289aa9f1f1d9ad7bcee694293c7623affb5979bd3f844ab4adcf2145b117\
            b7811b3cee31e130efd760e9685f208c2b2fb1d67e28262168013ba63c\"}]");
    }
}
