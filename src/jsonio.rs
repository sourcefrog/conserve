// Conserve backup system.
// Copyright 2015, 2016, 2018, 2020 Martin Pool.

//! Read and write JSON files.

use std::io::prelude::*;
use std::path::Path;

use snafu::ResultExt;

use super::io::AtomicFile;
use super::*;

pub fn write_json_metadata_file<T: serde::Serialize>(path: &Path, obj: &T) -> Result<()> {
    let mut af = AtomicFile::new(path).context(errors::WriteMetadata { path })?;
    let mut s = serde_json::to_string(&obj).context(errors::SerializeJson { path })?;
    s.push('\n');
    af.write_all(s.as_bytes())
        .context(errors::WriteMetadata { path })?;
    af.close().context(errors::WriteMetadata { path })?;
    Ok(())
}

pub fn read_json_metadata_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let buf = std::fs::read_to_string(&path).context(errors::ReadMetadata { path })?;
    serde_json::from_str(&buf).context(DeserializeJson { path })
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::test_fixtures::TreeFixture;

    #[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
    pub struct TestContents {
        pub id: u64,
        pub weather: String,
    }

    #[test]
    pub fn read_write_json() {
        let tree = TreeFixture::new();
        let entry = TestContents {
            id: 42,
            weather: "cold".to_string(),
        };
        let p = tree.path().join("test.json");
        write_json_metadata_file(&p, &entry).unwrap();
        let r: TestContents = read_json_metadata_file(&p).unwrap();
        assert_eq!(r, entry);
    }
}
