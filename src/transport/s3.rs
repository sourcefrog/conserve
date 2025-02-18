// Copyright 2023-2025 Martin Pool.

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

use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use aws_config::{AppName, BehaviorVersion};
use aws_sdk_s3::error::{DisplayErrorContext, ProvideErrorMetadata, SdkError};
use aws_sdk_s3::operation::delete_object::DeleteObjectError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use aws_sdk_s3::operation::list_objects_v2::{ListObjectsV2Error, ListObjectsV2Output};
use aws_sdk_s3::operation::put_object::PutObjectError;
use aws_sdk_s3::primitives::ByteStreamError;
use aws_sdk_s3::types::StorageClass;
use aws_types::region::Region;
use aws_types::SdkConfig;
use base64::Engine;
use bytes::Bytes;
use tracing::{debug, error, trace};
use url::Url;

use super::{Error, ErrorKind, Kind, ListDir, Metadata, Result, WriteMode};

pub(super) struct Protocol {
    url: Url,

    client: Arc<aws_sdk_s3::Client>,

    bucket: String,
    base_path: String,

    /// Storage class for new objects.
    storage_class: StorageClass,
}

impl Protocol {
    pub(super) async fn new(url: &Url) -> Result<Self> {
        assert_eq!(url.scheme(), "s3");
        let url = url.to_owned();

        let bucket = url.authority().to_owned();
        assert!(!bucket.is_empty(), "S3 bucket name is empty in {url:?}");

        // Find the bucket region.
        let config = load_aws_config(None).await;
        let client = aws_sdk_s3::Client::new(&config);
        let location_response = client
            .get_bucket_location()
            .set_bucket(Some(bucket.clone()))
            .send()
            .await
            .map_err(|err| s3_error(err, &url))?;
        debug!(?location_response);

        let region = location_response
            .location_constraint
            .map(|c| c.as_str().to_owned());
        debug!(?region, "S3 bucket region");

        // Make a new client in the right region.
        let config = load_aws_config(region).await;
        let client = aws_sdk_s3::Client::new(&config);

        let mut base_path = url.path().to_owned();
        if !base_path.is_empty() {
            base_path = base_path
                .strip_prefix('/')
                .expect("URL path starts with /")
                .trim_end_matches('/')
                .to_owned();
        }
        debug!(%bucket, %base_path);

        Ok(Protocol {
            bucket,
            base_path,
            client: Arc::new(client),
            storage_class: StorageClass::IntelligentTiering,
            url,
        })
    }

    fn join_path(&self, relpath: &str) -> String {
        join_paths(&self.base_path, relpath)
    }

    fn s3_error<E, R>(&self, key: &str, source: SdkError<E, R>) -> Error
    where
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
            url: self.url.join(key).ok(),
            source: Some(source.into()),
        }
    }
}

impl fmt::Debug for Protocol {
    #[mutants::skip] // unimportant to test
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("conserve::transport::s3::Protocol")
            .field("bucket", &self.bucket)
            .field("base_path", &self.base_path)
            .finish()
    }
}

async fn load_aws_config(region: Option<String>) -> SdkConfig {
    // Use us-east-1 at least for looking up the bucket's region, if
    // none is known yet.
    let loader = aws_config::defaults(BehaviorVersion::latest())
        .app_name(AppName::new(format!("conserve-{}", crate::version())).unwrap())
        .region(Region::new(
            region.unwrap_or_else(|| "us-east-1".to_owned()),
        ));
    loader.load().await
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

#[async_trait]
impl super::Protocol for Protocol {
    async fn list_dir_async(&self, relpath: &str) -> Result<ListDir> {
        trace!(%relpath, "list_dir");
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
        while let Some(response) = stream.next().await {
            let response = response.map_err(|err| self.s3_error(&prefix, err))?;
            collect_listdir(&response, &prefix, &mut result);
        }
        trace!(
            %relpath,
            n_dirs = result.dirs.len(),
            n_files = result.files.len(),
            "list_dir complete"
        );
        Ok(result)
    }

    async fn read(&self, relpath: &str) -> Result<Bytes> {
        trace!(?relpath, "s3::read");
        let key = self.join_path(relpath);
        let request = self.client.get_object().bucket(&self.bucket).key(&key);
        let response = request
            .send()
            .await
            .map_err(|source| self.s3_error(&key, source))?;
        let body_bytes = response
            .body
            .collect()
            .await
            .map_err(|source| Error {
                kind: ErrorKind::Other,
                url: self.url.join(relpath).ok(),
                source: Some(Box::new(source)),
            })?
            .into_bytes();
        trace!(body_len = body_bytes.len(), "read file");
        Ok(body_bytes)
    }

    #[mutants::skip] // does nothing so hard to observe!
    async fn create_dir(&self, relpath: &str) -> Result<()> {
        // There are no directory objects, so there's nothing to create.
        let _ = relpath;
        Ok(())
    }

    async fn write(&self, relpath: &str, content: &[u8], write_mode: WriteMode) -> Result<()> {
        let key = self.join_path(relpath);
        let crc32c =
            base64::engine::general_purpose::STANDARD.encode(crc32c::crc32c(content).to_be_bytes());
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .storage_class(self.storage_class.clone())
            .checksum_crc32_c(crc32c)
            .body(content.to_owned().into());
        if write_mode == WriteMode::CreateNew {
            request = request.if_none_match("*");
        }
        request
            .send()
            .await
            .map_err(|err| self.s3_error(&key, err))?;
        trace!(body_len = content.len(), "wrote file");
        Ok(())
    }

    async fn remove_file(&self, relpath: &str) -> Result<()> {
        trace!(%relpath, "S3Transport::remove_file");
        let key = self.join_path(relpath);
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map(|response| trace!(?response))
            .map_err(|err| self.s3_error(&key, err))
    }

    async fn remove_dir_all(&self, relpath: &str) -> Result<()> {
        // Walk the prefix and delete every object within it.
        // This could be locally parallelized, but it's only used during `conserve delete`
        // which isn't the most important thing to optimize.
        trace!(%relpath, "S3Transport::remove_dir_all");
        let prefix = self.join_path(relpath);
        let mut stream = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .into_paginator()
            .send();
        let mut n_files = 0;
        while let Some(response) = stream.next().await {
            for object in response
                .map_err(|err| self.s3_error(&prefix, err))?
                .contents
                .expect("ListObjectsV2Response has contents")
            {
                let key = object.key.expect("Object has a key");
                self.client
                    .delete_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .send()
                    .await
                    .map_err(|err| self.s3_error(&key, err))?;
                n_files += 1;
            }
        }
        trace!(n_files, "Deleted all files");
        Ok(())
    }

    async fn metadata(&self, relpath: &str) -> Result<Metadata> {
        let key = self.join_path(relpath);
        trace!(?key, "s3::metadata");
        let request = self.client.head_object().bucket(&self.bucket).key(&key);
        let response = request.send().await;
        // trace!(?response);
        match response {
            Ok(response) => {
                // TODO: Soft errors on unexpected API responses
                let len = response
                    .content_length
                    .expect("S3 HeadObject response should have a content_length")
                    .try_into()
                    .expect("Content length non-negative");
                let modified: SystemTime = response
                    .last_modified
                    .expect("S3 HeadObject response should have a last_modified")
                    .try_into()
                    .expect("S3 last_modified is valid SystemTime");
                trace!(?len, "File exists");
                Ok(Metadata {
                    kind: Kind::File,
                    len,
                    modified: modified.into(),
                })
            }
            Err(err) => {
                let translated = self.s3_error(&key, err);
                if translated.is_not_found() {
                    trace!("file does not exist");
                } else {
                    trace!(?translated, "error getting metadata");
                }
                Err(translated)
            }
        }
    }

    fn chdir(&self, relpath: &str) -> Arc<dyn super::Protocol> {
        Arc::new(Protocol {
            base_path: join_paths(&self.base_path, relpath),
            bucket: self.bucket.clone(),
            client: self.client.clone(),
            storage_class: self.storage_class.clone(),
            url: self.url.join(relpath).expect("join URL"),
        })
    }

    fn url(&self) -> &Url {
        &self.url
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

fn collect_listdir(response: &ListObjectsV2Output, prefix: &str, list_dir: &mut ListDir) {
    for common_prefix in response.common_prefixes() {
        let name = common_prefix
            .prefix
            .as_ref()
            .expect("Common prefix has a name"); // needed for lifetime
        trace!(%name, "S3 common prefix");
        let name = name
            .strip_prefix(prefix)
            .expect("Common prefix starts with prefix")
            .strip_suffix('/')
            .expect("Common prefix ends with /");
        debug_assert!(!name.contains('/'), "{name:?} contains / but shouldn't");
        list_dir.dirs.push(name.to_owned());
    }
    for object in response.contents() {
        let name = object.key.as_ref().expect("Object has a key"); // needed
        trace!(%name, "S3 object");
        let name = name
            .strip_prefix(prefix)
            .expect("Object name should start with prefix");
        debug_assert!(!name.contains('/'), "{name:?} contains / but shouldn't");
        list_dir.files.push(name.to_owned());
    }
}

fn s3_error<E: StdError + Sync + Send + 'static>(err: SdkError<E>, url: &Url) -> Error {
    // TODO: Break out more specific errors?
    //
    // For example:
    // ERROR conserve::transport::s3: S3 error: DisplayErrorContext(DispatchFailure(DispatchFailure { source: ConnectorError { kind: Other(None), source: CredentialsNotLoaded(CredentialsNotLoaded { source: Some("no providers in chain provided credentials") }), connection: Unknown } }))
    error!("S3 error: {:?}", DisplayErrorContext(&err));
    Error {
        source: Some(Box::new(err)),
        url: Some(url.to_owned()),
        kind: ErrorKind::Connect,
    }
}
