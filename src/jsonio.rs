// Conserve backup system.
// Copyright 2015, 2016, 2018, 2020 Martin Pool.

//! Read and write JSON files.

use std::io::prelude::*;
use std::path::Path;

use crate::errors::Error;
use crate::io::AtomicFile;
use crate::Result;

pub fn write_json_metadata_file<T: serde::Serialize>(path: &Path, obj: &T) -> Result<()> {
    let mut s: String = serde_json::to_string(&obj).map_err(|source| Error::SerializeJson {
        path: path.to_owned(),
        source,
    })?;
    s.push('\n');
    AtomicFile::new(path)
        .and_then(|mut af| {
            af.write_all(s.as_bytes())?;
            af.close()
        })
        .map_err(|source| Error::WriteMetadata {
            path: path.to_owned(),
            source,
        })
}

pub fn read_json_metadata_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let buf = std::fs::read_to_string(&path).map_err(|source| Error::ReadMetadata {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&buf).map_err(|source| Error::DeserializeJson {
        source,
        path: path.to_owned(),
    })
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
