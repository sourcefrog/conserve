// Conserve backup system.
// Copyright 2015, 2016, 2018, 2020, 2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Read and write JSON files.

use serde::de::DeserializeOwned;

use crate::errors::Error;
use crate::transport::Transport;
use crate::Result;

/// Write uncompressed json to a file on a Transport.
pub(crate) fn write_json<T, TR>(transport: &TR, relpath: &str, obj: &T) -> Result<()>
where
    T: serde::Serialize,
    TR: AsRef<dyn Transport>,
{
    let mut s: String = serde_json::to_string(&obj).map_err(|source| Error::SerializeJson {
        path: relpath.to_string(),
        source,
    })?;
    s.push('\n');
    transport
        .as_ref()
        .write_file(relpath, s.as_bytes())
        .map_err(|source| Error::WriteMetadata {
            path: relpath.to_owned(),
            source,
        })
}

/// Read and deserialize uncompressed json from a file on a Transport.
///
/// Returns None if the file does not exist.
pub(crate) fn read_json<T, TR>(transport: &TR, path: &str) -> Result<Option<T>>
where
    T: DeserializeOwned,
    TR: AsRef<dyn Transport>,
{
    let bytes = match transport.as_ref().read_file(path) {
        Ok(b) => b,
        Err(err) => {
            if err.is_not_found() {
                return Ok(None);
            } else {
                return Err(err.into());
            }
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(t) => Ok(Some(t)),
        Err(source) => Err(Error::DeserializeJson {
            source,
            // TODO: Full path from the transport?
            path: path.into(),
        }),
    }
}

#[cfg(test)]
mod tests {
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
    fn write_json_to_transport() {
        let temp = assert_fs::TempDir::new().unwrap();
        let entry = TestContents {
            id: 42,
            weather: "cold".to_string(),
        };
        let filename = "test.json";

        let transport = LocalTransport::new(temp.path());
        super::write_json(&transport, filename, &entry).unwrap();

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
        let content: TestContents = read_json(&transport, "test.json")
            .expect("no error")
            .expect("file exists");

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
