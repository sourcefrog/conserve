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

use std::io;
use std::path::PathBuf;

use serde::de::DeserializeOwned;

use crate::transport::{self, Transport, WriteMode};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error")]
    Io {
        #[from]
        source: io::Error,
    },

    #[error("JSON serialization error")]
    Json {
        source: serde_json::Error,
        path: PathBuf,
    },

    #[error("Transport error")]
    Transport {
        #[from]
        source: transport::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Write uncompressed json to a file on a Transport.
pub(crate) fn write_json<T>(transport: &Transport, relpath: &str, obj: &T) -> Result<()>
where
    T: serde::Serialize,
{
    let mut s: String = serde_json::to_string(&obj).map_err(|source| Error::Json {
        source,
        path: relpath.into(),
    })?;
    s.push('\n');
    transport
        .write(relpath, s.as_bytes(), WriteMode::CreateNew)
        .map_err(Error::from)
}

/// Read and deserialize uncompressed json from a file on a Transport.
///
/// Returns None if the file does not exist.
pub(crate) async fn read_json<T>(transport: &Transport, path: &str) -> Result<Option<T>>
where
    T: DeserializeOwned,
{
    let bytes = match transport.read_async(path).await {
        Ok(b) => b,
        Err(err) if err.is_not_found() => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    serde_json::from_slice(&bytes)
        .map(|t| Some(t))
        .map_err(|source| Error::Json {
            source,
            // TODO: Full path from the transport?
            path: path.into(),
        })
}

#[cfg(test)]
mod tests {
    use assert_fs::prelude::*;
    use serde::{Deserialize, Serialize};

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

        let transport = Transport::local(temp.path());
        super::write_json(&transport, filename, &entry).unwrap();

        let json_child = temp.child("test.json");
        json_child.assert(concat!(r#"{"id":42,"weather":"cold"}"#, "\n"));

        temp.close().unwrap();
    }

    #[tokio::test]
    async fn read_json_from_transport() {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child("test.json")
            .write_str(r#"{"id": 42, "weather": "cold"}"#)
            .unwrap();

        let transport = Transport::local(temp.path());
        let content: TestContents = read_json(&transport, "test.json")
            .await
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

    #[cfg(unix)]
    #[tokio::test]
    async fn read_json_error_when_permission_denied() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let transport = Transport::temp();
        let f = std::fs::File::create(transport.local_path().unwrap().join("file"))?;
        let metadata = f.metadata()?;
        let mut perms = metadata.permissions();
        perms.set_mode(0);
        f.set_permissions(perms)?;
        read_json::<TestContents>(&transport, "file")
            .await
            .expect_err("Read file with access denied");
        Ok(())
    }

    #[tokio::test]
    async fn read_json_is_none_for_nonexistent_files() {
        let transport = Transport::temp();
        assert!(read_json::<TestContents>(&transport, "nonexistent.json")
            .await
            .expect("No error for nonexistent file")
            .is_none());
    }
}
