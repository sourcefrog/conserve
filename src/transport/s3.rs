// Copyright 2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Access to an archive on AWS S3, or compatible object storage.

use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use tokio::runtime::Runtime;
use url::Url;

use super::{Error, ListDir, Metadata, Result, Transport};

#[derive(Debug)]
#[allow(dead_code)]
pub struct S3Transport {
    /// Tokio runtime specifically for S3 IO.
    runtime: Runtime,

    client: aws_sdk_s3::Client,

    base_url: Url,
}

impl S3Transport {
    pub fn new(base_url: &Url) -> Result<Arc<Self>> {
        // Like in <https://tokio.rs/tokio/topics/bridging>.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| Error::io_error(Path::new(""), err))?;
        let config = runtime.block_on(aws_config::load_from_env());
        let client = aws_sdk_s3::Client::new(&config);
        Ok(Arc::new(S3Transport {
            base_url: base_url.clone(),
            client,
            runtime,
        }))
    }
}

#[allow(unused_variables)]
impl Transport for S3Transport {
    fn list_dir(&self, relpath: &str) -> Result<ListDir> {
        todo!()
    }

    fn read_file(&self, relpath: &str) -> Result<Bytes> {
        // increment_counter!("conserve.local_transport.read));
        // let mut file = File::open(self.full_path(relpath))?;
        // let estimated_len: usize = file.metadata()?.len().try_into().unwrap();
        // let mut out_buf = Vec::with_capacity(estimated_len);
        // let actual_len = file.read_to_end(&mut out_buf)?;
        // counter!(
        //     "conserve.local_transport.read_file_bytes",
        //     actual_len as u64
        // );
        // out_buf.truncate(actual_len);
        // Ok(out_buf.into())
        todo!()
    }

    fn is_file(&self, relpath: &str) -> Result<bool> {
        // increment_counter!("conserve.local_transport.metadata_reads");
        // Ok(self.full_path(relpath).is_file())
        todo!()
    }

    fn create_dir(&self, relpath: &str) -> Result<()> {
        // create_dir(self.full_path(relpath)).or_else(|err| {
        //     if err.kind() == io::ErrorKind::AlreadyExists {
        //         Ok(())
        //     } else {
        //         Err(err)
        //     }
        // })
        todo!()
    }

    fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()> {
        // increment_counter!("conserve.local_transport.write_files");
        // counter!(
        //     "conserve.local_transport.write_file_bytes",
        //     content.len() as u64
        // );
        // let full_path = self.full_path(relpath);
        // let dir = full_path.parent().unwrap();
        // let mut temp = tempfile::Builder::new()
        //     .prefix(crate::TMP_PREFIX)
        //     .tempfile_in(dir)?;
        // if let Err(err) = temp.write_all(content) {
        //     let _ = temp.close();
        //     return Err(err);
        // }
        // if let Err(persist_error) = temp.persist(&full_path) {
        //     persist_error.file.close()?;
        //     Err(persist_error.error)
        // } else {
        //     Ok(())
        // }
        todo!()
    }

    fn remove_file(&self, relpath: &str) -> Result<()> {
        todo!()
        // std::fs::remove_file(self.full_path(relpath))
    }

    fn remove_dir_all(&self, relpath: &str) -> Result<()> {
        todo!()
        // std::fs::remove_dir_all(self.full_path(relpath))
    }

    fn sub_transport(&self, relpath: &str) -> Arc<dyn Transport> {
        todo!()
        // Box::new(S3Transport {
        //     root: self.root.join(relpath),
        // })
    }

    fn metadata(&self, relpath: &str) -> Result<Metadata> {
        // increment_counter!("conserve.local_transport.metadata_reads");
        // let fsmeta = self.root.join(relpath).metadata()?;
        // Ok(Metadata {
        //     len: fsmeta.len(),
        //     kind: fsmeta.file_type().into(),
        // })
        todo!()
    }

    fn url_scheme(&self) -> &'static str {
        "file"
    }
}

impl AsRef<dyn Transport> for S3Transport {
    fn as_ref(&self) -> &(dyn Transport + 'static) {
        self
    }
}

#[cfg(test)]
mod test {
    use assert_fs::prelude::*;
    use predicates::prelude::*;

    use super::*;
    use crate::kind::Kind;

    // #[test]
    // fn read_file() {
    //     let temp = assert_fs::TempDir::new().unwrap();
    //     let content: &str = "the ribs of the disaster";
    //     let filename = "poem.txt";

    //     temp.child(filename).write_str(content).unwrap();

    //     let transport = S3Transport::new(temp.path());
    //     let buf = transport.read_file(filename).unwrap();
    //     assert_eq!(buf, content.as_bytes());

    //     temp.close().unwrap();
    // }

    // #[test]
    // fn read_metadata() {
    //     let temp = assert_fs::TempDir::new().unwrap();
    //     let content: &str = "the ribs of the disaster";
    //     let filename = "poem.txt";
    //     temp.child(filename).write_str(content).unwrap();

    //     let transport = S3Transport::new(temp.path());

    //     assert_eq!(
    //         transport.metadata(filename).unwrap(),
    //         Metadata {
    //             len: 24,
    //             kind: Kind::File
    //         }
    //     );
    //     assert!(transport.metadata("nopoem").is_err());
    // }

    // #[test]
    // fn list_directory() {
    //     let temp = assert_fs::TempDir::new().unwrap();
    //     temp.child("root file").touch().unwrap();
    //     temp.child("subdir").create_dir_all().unwrap();
    //     temp.child("subdir")
    //         .child("subfile")
    //         .write_str("Morning coffee")
    //         .unwrap();

    //     let transport = S3Transport::new(temp.path());
    //     let mut root_list: Vec<_> = transport
    //         .iter_dir_entries(".")
    //         .unwrap()
    //         .map(std::Result::unwrap)
    //         .collect();
    //     assert_eq!(root_list.len(), 2);
    //     root_list.sort();

    //     assert_eq!(
    //         root_list[0],
    //         DirEntry {
    //             name: "root file".to_owned(),
    //             kind: Kind::File,
    //         }
    //     );

    //     // Len is unpredictable for directories, so check the other fields.
    //     assert_eq!(root_list[1].name, "subdir");
    //     assert_eq!(root_list[1].kind, Kind::Dir);

    //     assert!(transport.is_file("root file").unwrap());
    //     assert!(!transport.is_file("nuh-uh").unwrap());

    //     let subdir_list: Vec<_> = transport
    //         .iter_dir_entries("subdir")
    //         .unwrap()
    //         .map(std::Result::unwrap)
    //         .collect();
    //     assert_eq!(
    //         subdir_list,
    //         vec![DirEntry {
    //             name: "subfile".to_owned(),
    //             kind: Kind::File,
    //         }]
    //     );

    //     temp.close().unwrap();
    // }

    // #[test]
    // fn write_file() {
    //     // TODO: Maybe test some error cases of failing to write.
    //     let temp = assert_fs::TempDir::new().unwrap();
    //     let transport = S3Transport::new(temp.path());

    //     transport.create_dir("subdir").unwrap();
    //     transport
    //         .write_file("subdir/subfile", b"Must I paint you a picture?")
    //         .unwrap();

    //     temp.child("subdir").assert(predicate::path::is_dir());
    //     temp.child("subdir")
    //         .child("subfile")
    //         .assert("Must I paint you a picture?");

    //     temp.close().unwrap();
    // }

    // #[test]
    // fn create_existing_dir() {
    //     let temp = assert_fs::TempDir::new().unwrap();
    //     let transport = S3Transport::new(temp.path());

    //     transport.create_dir("aaa").unwrap();
    //     transport.create_dir("aaa").unwrap();
    //     transport.create_dir("aaa").unwrap();

    //     temp.close().unwrap();
    // }

    // #[test]
    // fn sub_transport() {
    //     let temp = assert_fs::TempDir::new().unwrap();
    //     let transport = S3Transport::new(temp.path());

    //     transport.create_dir("aaa").unwrap();
    //     transport.create_dir("aaa/bbb").unwrap();

    //     let sub_transport = transport.sub_transport("aaa");
    //     let sub_list: Vec<DirEntry> = sub_transport
    //         .iter_dir_entries("")
    //         .unwrap()
    //         .map(|r| r.unwrap())
    //         .collect();

    //     assert_eq!(sub_list.len(), 1);
    //     assert_eq!(sub_list[0].name, "bbb");

    //     temp.close().unwrap();
    // }

    // #[test]
    // fn remove_dir_all() -> std::Result<()> {
    //     let temp = assert_fs::TempDir::new().unwrap();
    //     let transport = S3Transport::new(temp.path());

    //     transport.create_dir("aaa")?;
    //     transport.create_dir("aaa/bbb")?;
    //     transport.create_dir("aaa/bbb/ccc")?;

    //     transport.remove_dir_all("aaa")?;
    //     Ok(())
    // }
}
