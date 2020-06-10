// Conserve backup system.
// Copyright 2015, 2016, 2018, 2020 Martin Pool.

//! Read and write JSON files.

use std::io::prelude::*;
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::errors::Error;
use crate::io::AtomicFile;
use crate::transport::{TransportRead, TransportWrite};
use crate::Result;

pub(crate) fn write_json_metadata_file<T: serde::Serialize>(path: &Path, obj: &T) -> Result<()> {
    let mut s: String = serde_json::to_string(&obj).map_err(|source| Error::SerializeJson {
        path: path.to_string_lossy().to_string(),
        source,
    })?;
    s.push('\n');
    AtomicFile::new(path)
        .and_then(|mut af| {
            af.write_all(s.as_bytes())?;
            af.close()
        })
        .map_err(|source| Error::WriteMetadata {
            path: path.to_string_lossy().to_string(),
            source,
        })
}

/// Write uncompressed json to a file on a Transport.
pub(crate) fn write_json<T>(
    transport: &mut dyn TransportWrite,
    relpath: &str,
    obj: &T,
) -> Result<()>
where
    T: serde::Serialize,
{
    let mut s: String = serde_json::to_string(&obj).map_err(|source| Error::SerializeJson {
        path: relpath.to_string(),
        source,
    })?;
    s.push('\n');
    transport
        .write_file(relpath, s.as_bytes())
        .map_err(|source| Error::WriteMetadata {
            path: relpath.to_owned(),
            source,
        })
}

/// Read and deserialize uncompressed json from a Transport.
pub(crate) fn read_json<T>(transport: &dyn TransportRead, path: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut buf = Vec::new();
    transport.read_file(path, &mut buf).map_err(Error::from)?;
    serde_json::from_slice(&buf).map_err(|source| Error::DeserializeJson {
        source,
        path: path.into(),
    })
}

#[cfg(test)]
mod tests {
    use assert_fs;
    use assert_fs::prelude::*;
    use serde::{Deserialize, Serialize};

    use crate::transport::local::LocalTransport;

    use super::*;

    #[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
    struct TestContents {
        pub id: u64,
        pub weather: String,
    }

    #[test]
    fn write_json() {
        let temp = assert_fs::TempDir::new().unwrap();
        let entry = TestContents {
            id: 42,
            weather: "cold".to_string(),
        };
        let json_child = temp.child("test.json");
        write_json_metadata_file(&json_child.path(), &entry).unwrap();

        json_child.assert(concat!(r#"{"id":42,"weather":"cold"}"#, "\n"));

        temp.close().unwrap();
    }

    #[test]
    fn write_json_to_transport() {
        let temp = assert_fs::TempDir::new().unwrap();
        let entry = TestContents {
            id: 42,
            weather: "cold".to_string(),
        };
        let filename = "test.json";

        let mut transport = LocalTransport::new(&temp.path());
        super::write_json(&mut transport, filename, &entry).unwrap();

        let json_child = temp.child("test.json");
        json_child.assert(concat!(r#"{"id":42,"weather":"cold"}"#, "\n"));

        temp.close().unwrap();
    }

    #[test]
    fn read_json_from_transport() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child("test.json")
            .write_str(r#"{"id": 42, "weather": "cold"}"#)
            .unwrap();

        let transport = LocalTransport::new(temp.path());
        let content: TestContents = read_json(&transport, "test.json").unwrap();

        assert_eq!(
            content,
            TestContents {
                id: 42,
                weather: "cold".to_owned()
            }
        );

        temp.close().unwrap();
    }
}
