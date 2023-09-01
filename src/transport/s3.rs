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

use aws_config::AppName;
use aws_sdk_s3::error::SdkError;
use aws_types::region::Region;
use aws_types::SdkConfig;
use bytes::Bytes;
use futures::stream::StreamExt;
use tokio::runtime::Runtime;
use tracing::{debug, trace, trace_span};
use url::Url;

use super::{Error, ErrorKind, Kind, ListDir, Metadata, Result, Transport};

#[derive(Debug)]
#[allow(dead_code)]
pub struct S3Transport {
    /// Tokio runtime specifically for S3 IO.
    runtime: Arc<Runtime>,

    client: Arc<aws_sdk_s3::Client>,

    bucket: String,
    base_path: String,
}

impl S3Transport {
    pub fn new(base_url: &Url) -> Result<Arc<Self>> {
        // Like in <https://tokio.rs/tokio/topics/bridging>.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| Error::io_error(Path::new(""), err))?;

        let bucket = base_url.authority().to_owned();
        assert!(
            !bucket.is_empty(),
            "S3 bucket name is empty in {base_url:?}"
        );

        // Find the bucket region.
        let config = load_aws_config(&runtime, None);
        let client = aws_sdk_s3::Client::new(&config);
        let location_request = client
            .get_bucket_location()
            .set_bucket(Some(bucket.clone()));
        let location_response = runtime
            .block_on(location_request.send())
            .expect("Send GetBucketLocation");
        debug!(?location_response);

        let region = location_response
            .location_constraint
            .map(|c| c.as_str().to_owned());
        debug!(?region, "S3 bucket region");

        // Make a new client in the right region.
        let config = load_aws_config(&runtime, region);
        let client = aws_sdk_s3::Client::new(&config);

        let base_path = base_url.path().trim_end_matches('/').to_owned();
        debug!(%bucket, %base_path);

        Ok(Arc::new(S3Transport {
            bucket,
            base_path,
            client: Arc::new(client),
            runtime: Arc::new(runtime),
        }))
    }
}

fn load_aws_config(runtime: &Runtime, region: Option<String>) -> SdkConfig {
    let mut loader = aws_config::from_env()
        .app_name(AppName::new(format!("conserve-{}", crate::version())).unwrap());
    if let Some(region) = region {
        loader = loader.region(Region::new(region));
    }
    runtime.block_on(loader.load())
}

/// Join paths in a way that works for S3 keys.
///
/// S3 doesn't have directories, only keys that can contain slashes. So we
/// have to be more careful not to produce double slashes or to insert an
/// extra slash at the start.
fn join_paths(a: &str, b: &str) -> String {
    if b.is_empty() {
        return a.to_owned();
    }
    if a.is_empty() {
        return b.to_owned();
    }
    let mut result = a.to_owned();
    if !result.ends_with('/') {
        result.push('/');
    }
    result.push_str(b);
    debug_assert!(
        !result.contains("//"),
        "result must not contain //: {result:?}"
    );
    debug_assert!(
        !result.starts_with('/'),
        "result must not start with /: {result:?}"
    );
    debug_assert!(
        !result.contains("/../"),
        "result must not contain /../: {result:?}"
    );
    debug_assert!(
        !result.ends_with('/'),
        "result must not end with /: {result:?}"
    );
    result
}

#[allow(unused_variables)]
impl Transport for S3Transport {
    fn list_dir(&self, relpath: &str) -> Result<ListDir> {
        let _span = trace_span!("S3Transport::list_file", %relpath).entered();
        let prefix = self.join_path(relpath);
        let mut stream = self
            .client
            .list_objects_v2()
            .set_bucket(Some(self.bucket.clone()))
            .set_prefix(Some(prefix.clone()))
            .set_delimiter(Some("/".to_owned()))
            .into_paginator()
            .send();
        let mut result = ListDir::default();
        loop {
            match self.runtime.block_on(stream.next()) {
                Some(Ok(response)) => {
                    for common_prefix in response.common_prefixes.unwrap_or_default() {
                        let name = common_prefix.prefix.expect("Common prefix has a name");
                        debug!(%name, "S3 common prefix");
                        let name = name
                            .strip_prefix(&prefix)
                            .expect("Common prefix starts with prefix")
                            .strip_suffix('/')
                            .expect("Common prefix ends with /");
                        debug_assert!(!name.contains('/'), "{name:?} contains / but shouldn't");
                        result.dirs.push(name.to_owned());
                    }
                    for object in response.contents.unwrap_or_default() {
                        let name = object.key.expect("Object has a key");
                        debug!(%name, "S3 object");
                        let name = name
                            .strip_prefix(&prefix)
                            .expect("Object name should start with prefix");
                        debug_assert!(!name.contains('/'), "{name:?} contains / but shouldn't");
                        result.files.push(name.to_owned());
                    }
                }
                Some(Err(err)) => panic!("S3 request failed: {}", err), // TODO: Return Err
                None => break,
            }
        }
        Ok(result)
    }

    fn read_file(&self, relpath: &str) -> Result<Bytes> {
        let _span = trace_span!("S3Transport::read_file", %relpath).entered();
        let key = self.join_path(relpath);
        let request = self
            .client
            .get_object()
            .set_bucket(Some(self.bucket.clone()))
            .set_key(Some(key));
        let response = self
            .runtime
            .block_on(request.send())
            .expect("S3 request succeeded"); // TODO: No panic, map to error
        let body_bytes = self
            .runtime
            .block_on(response.body.collect())
            .expect("Read S3 response body");
        Ok(body_bytes.into_bytes())
    }

    fn create_dir(&self, relpath: &str) -> Result<()> {
        // There are no directory objects, so there's nothing to create.
        let _ = relpath;
        Ok(())
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

    fn metadata(&self, relpath: &str) -> Result<Metadata> {
        let _span = trace_span!("S3Transport::metadata", %relpath).entered();
        let key = self.join_path(relpath);
        let request = self.client.head_object().bucket(&self.bucket).key(&key);
        let response = self.runtime.block_on(request.send());
        trace!(?response);
        match response {
            Ok(response) => Ok(Metadata {
                kind: Kind::File,
                len: response
                    .content_length
                    .try_into()
                    .expect("content length non-negative"),
            }),
            Err(err) => match &err {
                SdkError::ServiceError(service_err) if service_err.err().is_not_found() => {
                    Err(Error {
                        path: Some(key),
                        kind: ErrorKind::NotFound,
                        source: Some(Box::new(err)),
                    })
                }
                other => todo!("Unhandled S3 error: {other:#?}"), // TODO: Return Err
            },
        }
    }

    fn sub_transport(&self, relpath: &str) -> Arc<dyn Transport> {
        Arc::new(S3Transport {
            base_path: join_paths(&self.base_path, relpath),
            bucket: self.bucket.clone(),
            runtime: self.runtime.clone(),
            client: self.client.clone(),
        })
    }

    fn url_scheme(&self) -> &'static str {
        "s3"
    }
}

impl S3Transport {
    fn join_path(&self, relpath: &str) -> String {
        join_paths(&self.base_path, relpath)
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
