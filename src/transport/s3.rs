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

// This file is mostly tested by the s3-integration test, which needs
// AWS credentials and so is not built or run by default.
//
// To run it, use
//
//     cargo test --features s3-integration-test,s3 --test s3-integration
//
// Similarly, this file is not included in mutation testing by default,
// but it can be tested with
//
//    cargo mutants -f s3.rs --no-config -C --features=s3,s3-integration-test

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use aws_config::AppName;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::delete_object::DeleteObjectError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error;
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::primitives::ByteStreamError;
use aws_sdk_s3::types::StorageClass;
use aws_types::region::Region;
use aws_types::SdkConfig;
use base64::Engine;
use bytes::Bytes;
use futures::stream::StreamExt;
use tokio::runtime::Runtime;
use tracing::{debug, trace, trace_span};
use url::Url;

use super::{Error, ErrorKind, Kind, ListDir, Metadata, Result, Transport};

pub struct S3Transport {
    /// Tokio runtime specifically for S3 IO.
    ///
    /// S3 SDK is built on Tokio but the rest of Conserve uses threads.
    /// Each call into the S3 transport blocks the calling thread
    /// until the request is complete.
    runtime: Arc<Runtime>,

    client: Arc<aws_sdk_s3::Client>,

    bucket: String,
    base_path: String,

    /// Storage class for new objects.
    storage_class: StorageClass,
}

impl fmt::Debug for S3Transport {
    #[mutants::skip] // unimportant to test
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Transport")
            .field("bucket", &self.bucket)
            .field("base_path", &self.base_path)
            .finish()
    }
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

        let mut base_path = base_url.path().to_owned();
        if !base_path.is_empty() {
            base_path = base_path
                .strip_prefix('/')
                .expect("URL path starts with /")
                .trim_end_matches('/')
                .to_owned();
        }
        debug!(%bucket, %base_path);

        Ok(Arc::new(S3Transport {
            bucket,
            base_path,
            client: Arc::new(client),
            runtime: Arc::new(runtime),
            storage_class: StorageClass::IntelligentTiering,
        }))
    }
}

fn load_aws_config(runtime: &Runtime, region: Option<String>) -> SdkConfig {
    // Use us-east-1 at least for looking up the bucket's region, if
    // none is known yet.
    let loader = aws_config::from_env()
        .app_name(AppName::new(format!("conserve-{}", crate::version())).unwrap())
        .region(Region::new(
            region.unwrap_or_else(|| "us-east-1".to_owned()),
        ));
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

impl Transport for S3Transport {
    fn list_dir(&self, relpath: &str) -> Result<ListDir> {
        let _span = trace_span!("S3Transport::list_file", %relpath).entered();
        let mut prefix = self.join_path(relpath);
        debug_assert!(!prefix.ends_with('/'), "{prefix:?} ends with /");
        if !prefix.is_empty() {
            prefix.push('/'); // add a slash to get the files inside this directory.
        }
        let mut stream = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .delimiter("/")
            .into_paginator()
            .send();
        let mut result = ListDir::default();
        loop {
            match self.runtime.block_on(stream.next()) {
                Some(Ok(response)) => {
                    for common_prefix in response.common_prefixes.unwrap_or_default() {
                        let name = common_prefix.prefix.expect("Common prefix has a name"); // needed for lifetime
                        trace!(%name, "S3 common prefix");
                        let name = name
                            .strip_prefix(&prefix)
                            .expect("Common prefix starts with prefix")
                            .strip_suffix('/')
                            .expect("Common prefix ends with /");
                        debug_assert!(!name.contains('/'), "{name:?} contains / but shouldn't");
                        result.dirs.push(name.to_owned());
                    }
                    for object in response.contents.unwrap_or_default() {
                        let name = object.key.expect("Object has a key"); // needed
                        trace!(%name, "S3 object");
                        let name = name
                            .strip_prefix(&prefix)
                            .expect("Object name should start with prefix");
                        debug_assert!(!name.contains('/'), "{name:?} contains / but shouldn't");
                        result.files.push(name.to_owned());
                    }
                }
                Some(Err(err)) => return Err(s3_error(prefix, err)),
                None => break,
            }
        }
        trace!(
            n_dirs = result.dirs.len(),
            n_files = result.files.len(),
            "list_dir complete"
        );
        Ok(result)
    }

    fn read_file(&self, relpath: &str) -> Result<Bytes> {
        let _span = trace_span!("S3Transport::read_file", %relpath).entered();
        let key = self.join_path(relpath);
        let request = self.client.get_object().bucket(&self.bucket).key(&key);
        let response = self
            .runtime
            .block_on(request.send())
            .map_err(|source| s3_error(key.clone(), source))?;
        let body_bytes = self
            .runtime
            .block_on(response.body.collect())
            .map_err(|source| Error {
                kind: ErrorKind::Other,
                path: Some(key.clone()),
                source: Some(Box::new(source)),
            })?
            .into_bytes();
        trace!(body_len = body_bytes.len(), "read file");
        Ok(body_bytes)
    }

    #[mutants::skip] // does nothing so hard to observe!
    fn create_dir(&self, relpath: &str) -> Result<()> {
        // There are no directory objects, so there's nothing to create.
        let _ = relpath;
        Ok(())
    }

    fn write_file(&self, relpath: &str, content: &[u8]) -> Result<()> {
        let _span = trace_span!("S3Transport::write_file", %relpath).entered();
        let key = self.join_path(relpath);
        let crc32c =
            base64::engine::general_purpose::STANDARD.encode(crc32c::crc32c(content).to_be_bytes());
        let request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .storage_class(self.storage_class.clone())
            .checksum_crc32_c(crc32c)
            .body(content.to_owned().into());
        let response = self.runtime.block_on(request.send());
        // trace!(?response);
        response.map_err(|err| s3_error(key, err))?;
        trace!(body_len = content.len(), "wrote file");
        Ok(())
    }

    fn remove_file(&self, relpath: &str) -> Result<()> {
        let _span = trace_span!("S3Transport::remove_file", %relpath).entered();
        let key = self.join_path(relpath);
        let request = self.client.delete_object().bucket(&self.bucket).key(&key);
        let response = self.runtime.block_on(request.send());
        trace!(?response);
        response.map_err(|err| s3_error(key, err))?;
        trace!("deleted file");
        Ok(())
    }

    fn remove_dir_all(&self, relpath: &str) -> Result<()> {
        // Walk the prefix and delete every object within it.
        // This could be locally parallelized, but it's only used during `conserve delete`
        // which isn't the most important thing to optimize.
        let _span = trace_span!("S3Transport::remove_dir_all", %relpath).entered();
        let prefix = self.join_path(relpath);
        let mut stream = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .into_paginator()
            .send();
        let mut n_files = 0;
        while let Some(response) = self.runtime.block_on(stream.next()) {
            for object in response
                .map_err(|err| s3_error(prefix.clone(), err))?
                .contents
                .expect("ListObjectsV2Response has contents")
            {
                let key = object.key.expect("Object has a key");
                self.runtime
                    .block_on(
                        self.client
                            .delete_object()
                            .bucket(&self.bucket)
                            .key(&key)
                            .send(),
                    )
                    .map_err(|err| s3_error(key, err))?;
                n_files += 1;
            }
        }
        trace!(n_files, "Deleted all files");
        Ok(())
    }

    fn metadata(&self, relpath: &str) -> Result<Metadata> {
        let _span = trace_span!("S3Transport::metadata", %relpath).entered();
        let key = self.join_path(relpath);
        let request = self.client.head_object().bucket(&self.bucket).key(&key);
        let response = self.runtime.block_on(request.send());
        // trace!(?response);
        match response {
            Ok(response) => {
                let len = response
                    .content_length
                    .try_into()
                    .expect("Content length non-negative");
                trace!(?len, "File exists");
                Ok(Metadata {
                    kind: Kind::File,
                    len,
                })
            }
            Err(err) => {
                let translated = s3_error(key, err);
                if translated.is_not_found() {
                    trace!("file does not exist");
                } else {
                    trace!(?translated, "error getting metadata");
                }
                Err(translated)
            }
        }
    }

    fn sub_transport(&self, relpath: &str) -> Arc<dyn Transport> {
        Arc::new(S3Transport {
            base_path: join_paths(&self.base_path, relpath),
            bucket: self.bucket.clone(),
            runtime: self.runtime.clone(),
            client: self.client.clone(),
            storage_class: self.storage_class.clone(),
        })
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

fn s3_error<K, E, R>(key: K, source: SdkError<E, R>) -> Error
where
    K: ToOwned<Owned = String>,
    E: std::error::Error + Send + Sync + 'static,
    R: std::fmt::Debug + Send + Sync + 'static,
    ErrorKind: for<'a> From<&'a E>,
{
    debug!(s3_error = ?source);
    let kind = match &source {
        SdkError::ServiceError(service_err) => ErrorKind::from(service_err.err()),
        _ => ErrorKind::Other,
    };
    Error {
        kind,
        path: Some(key.to_owned()),
        source: Some(source.into()),
    }
}

impl From<&GetObjectError> for ErrorKind {
    fn from(source: &GetObjectError) -> Self {
        match source {
            GetObjectError::NoSuchKey(_) => ErrorKind::NotFound,
            _ => ErrorKind::Other,
        }
    }
}

impl From<&ListObjectsV2Error> for ErrorKind {
    fn from(source: &ListObjectsV2Error) -> Self {
        match &source {
            ListObjectsV2Error::NoSuchBucket(_) => ErrorKind::NotFound,
            _ => ErrorKind::Other,
        }
    }
}

impl From<&PutObjectError> for ErrorKind {
    fn from(source: &PutObjectError) -> Self {
        let _ = source;
        ErrorKind::Other
    }
}

impl From<&HeadObjectError> for ErrorKind {
    fn from(source: &HeadObjectError) -> Self {
        match &source {
            HeadObjectError::NotFound(..) => ErrorKind::NotFound,
            _ => ErrorKind::Other,
        }
    }
}

impl From<&DeleteObjectError> for ErrorKind {
    fn from(_source: &DeleteObjectError) -> Self {
        // The AWS crate doesn't return a clear "not found" in this version.
        ErrorKind::Other
    }
}

impl From<&ByteStreamError> for ErrorKind {
    fn from(_source: &ByteStreamError) -> Self {
        ErrorKind::Other
    }
}
