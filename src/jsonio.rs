// Conserve backup system.
// Copyright 2015, 2016, 2018 Martin Pool.

//! Read and write JSON files.

use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use super::errors::*;
use super::io::AtomicFile;
use super::Report;

pub fn write_serde<T: serde::Serialize>(path: &Path, obj: &T, report: &Report) -> Result<()> {
    let mut f = AtomicFile::new(path)?;
    let mut s = serde_json::to_string(&obj).unwrap();
    s.push('\n');
    f.write_all(s.as_bytes())?;
    f.close(report)?;
    Ok(())
}

pub fn read_serde<T: serde::de::DeserializeOwned>(path: &Path, _report: &Report) -> Result<T> {
    // TODO: Send something to the Report.  At present this is used only for
    // small metadata files so measurement is not critical.
    let mut f = File::open(path).or_else(|e| Err(Error::IoError(e)))?;
    let mut buf = String::new();
    let _bytes_read = f.read_to_string(&mut buf)?;
    serde_json::from_str(&buf).or_else(|e| Err(e.into()))
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::TreeFixture;
    use crate::Report;

    #[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
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
        super::write_serde(&p, &entry, &write_report).unwrap();
        // NB: This does not currently do much with `report` other than measure timing.

        let read_report = Report::new();
        let r: TestContents = super::read_serde(&p, &read_report).unwrap();
        assert_eq!(r, entry);
    }
}
