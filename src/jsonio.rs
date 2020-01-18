// Conserve backup system.
// Copyright 2015, 2016, 2018, 2020 Martin Pool.

//! Read and write JSON files.

use std::io::prelude::*;
use std::path::Path;

use snafu::ResultExt;

use super::io::AtomicFile;
use super::Report;
use super::*;

pub fn write_json_metadata_file<T: serde::Serialize>(
    path: &Path,
    obj: &T,
    report: &Report,
) -> Result<()> {
    let mut f = AtomicFile::new(path).context(errors::WriteMetadata { path })?;
    let mut s = serde_json::to_string(&obj).context(errors::SerializeJson { path })?;
    s.push('\n');
    f.write_all(s.as_bytes())
        .context(errors::WriteMetadata { path })?;
    f.close(report).context(errors::WriteMetadata { path })?;
    Ok(())
}

pub fn read_json_metadata_file<T: serde::de::DeserializeOwned>(
    path: &Path,
    _report: &Report,
) -> Result<T> {
    // TODO: Send something to the Report.  At present this is used only for
    // small metadata files so measurement is not critical.
    let buf = std::fs::read_to_string(&path).context(errors::ReadMetadata { path })?;
    serde_json::from_str(&buf).context(DeserializeJson { path })
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

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
        super::write_json_metadata_file(&p, &entry, &write_report).unwrap();
        // NB: This does not currently do much with `report` other than measure timing.

        let read_report = Report::new();
        let r: TestContents = super::read_json_metadata_file(&p, &read_report).unwrap();
        assert_eq!(r, entry);
    }
}
