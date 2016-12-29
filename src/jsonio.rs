// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Read and write JSON files.

use std::fs::File;
use std::io::prelude::*;
use std::path::{Path};

use rustc_serialize::json;
use rustc_serialize::{Decodable, Encodable};

use super::Report;
use super::errors::*;
use super::io::AtomicFile;


pub fn write<T: Encodable>(path: &Path, obj: &T, report: &Report) -> Result<()> {
    let mut f = try!(AtomicFile::new(path));
    try!(f.write_all(json::encode(&obj).unwrap().as_bytes()));
    try!(f.write_all(b"\n"));
    try!(f.close(report));
    Ok(())
}


pub fn read<T: Decodable>(path: &Path, _report: &Report) -> Result<T> {
    // TODO: Send something to the Report.
    // At present this is used only for small metadata files so measurement is not
    // critical.
    let mut f = File::open(path).chain_err(|| format!("Failed to open {:?}", path))?;
    let mut buf = String::new();
    let _bytes_read = f.read_to_string(&mut buf)?;
    json::decode(&buf).chain_err(|| format!("Couldn't deserialize {:?}", path))
}


#[cfg(test)]
mod tests {
    use testfixtures::TreeFixture;
    use Report;

    #[derive(Debug, Eq, PartialEq, RustcDecodable, RustcEncodable)]
    pub struct TestContents {
        pub id: u64,
        pub weather: String,
    }

    #[test]
    pub fn read_write_json() {
        let tree = TreeFixture::new();
        let write_report = Report::new();
        let entry = TestContents {
            id: 42,
            weather: "cold".to_string(),
        };
        let p = tree.path().join("test.json");
        super::write(&p, &entry, &write_report).unwrap();
        // NB: This does not currently do much with `report` other than measure timing.

        let read_report = Report::new();
        let r: TestContents = super::read(&p, &read_report).unwrap();
        assert_eq!(r, entry);
    }
}
