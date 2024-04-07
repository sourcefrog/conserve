// Copyright 2023 Martin Pool

#![cfg(feature = "s3-integration-test")]

//! Test s3 transport, only when the `s3-integration-test`
//! feature is enabled.
//!
//! Run this with e.g.
//!
//!     cargo t --features=s3-integration-test --test s3-integration
//!
//! This must be run with AWS credentials available, e.g. in
//! the environment, because it writes to a real temporary bucket.
//!
//! A new bucket is created per test, with object expiry. This test will
//! attempt to delete the bucket when it stops, but this can't be guaranteed.

// This is (currently) written as explicit blocking calls on a runtime
// rather than "real" async, or making use or rstest's async features,
// to be more similar to the code under test.

use std::str::FromStr;

use ::aws_config::{AppName, BehaviorVersion};
use assert_cmd::Command;
use aws_sdk_s3::types::{
    BucketLifecycleConfiguration, BucketLocationConstraint, CreateBucketConfiguration,
    ExpirationStatus, LifecycleExpiration, LifecycleRule, LifecycleRuleFilter,
};
use indoc::indoc;
use rand::Rng;
use time::macros::format_description;
use time::OffsetDateTime;
use tokio::runtime::Runtime;

struct TempBucket {
    runtime: Runtime,
    bucket_name: String,
    client: aws_sdk_s3::Client,
}

impl TempBucket {
    pub fn url(&self) -> String {
        format!("s3://{}", self.bucket_name)
    }

    fn new() -> TempBucket {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Create runtime");
        let bucket_name = format!(
            "conserve-s3-integration-{time}-{rand:x}",
            time = OffsetDateTime::now_utc()
                .format(format_description!("[year][month][day]-[hour][minute]"))
                .expect("Format time"),
            rand = rand::thread_rng().gen::<u64>()
        );
        let app_name = AppName::new(format!(
            "conserve-s3-integration-test-{}",
            conserve::version()
        ))
        .unwrap();
        let config = runtime.block_on(
            ::aws_config::defaults(BehaviorVersion::latest())
                .app_name(app_name)
                .load(),
        );
        let client = aws_sdk_s3::Client::new(&config);
        runtime.block_on(TempBucket::setup_bucket(&bucket_name, &client));
        TempBucket {
            runtime,
            bucket_name,
            client,
        }
    }

    async fn setup_bucket(bucket_name: &str, client: &aws_sdk_s3::Client) {
        println!("make a bucket");
        let region = client
            .config()
            .region()
            .expect("AWS config from environment specifies a region")
            .as_ref();
        dbg!(&region);

        client
            .create_bucket()
            .bucket(bucket_name)
            .create_bucket_configuration(
                CreateBucketConfiguration::builder()
                    .location_constraint(BucketLocationConstraint::from_str(region).unwrap())
                    .build(),
            )
            .send()
            .await
            .expect("Create bucket");
        println!("Created bucket {bucket_name}");

        client
            .put_bucket_lifecycle_configuration()
            .bucket(bucket_name)
            .lifecycle_configuration(
                BucketLifecycleConfiguration::builder()
                    .rules(
                        LifecycleRule::builder()
                            .id("delete-after-7d")
                            .filter(LifecycleRuleFilter::ObjectSizeGreaterThan(0))
                            .status(ExpirationStatus::Enabled)
                            .expiration(LifecycleExpiration::builder().days(7).build())
                            .build()
                            .expect("Build S3 lifecycle rule"),
                    )
                    .build()
                    .expect("Build S3 lifecycle configuration"),
            )
            .send()
            .await
            .expect("Set bucket lifecycle");
        println!("Set lifecycle on bucket {bucket_name}");
    }

    /// Delete all objects and then the bucket.
    async fn delete(&self) {
        let mut paginator = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket_name)
            .into_paginator()
            .send();
        while let Some(page) = paginator.next().await {
            for object in page
                .expect("List objects page")
                .contents
                .unwrap_or_default()
            {
                self.client
                    .delete_object()
                    .bucket(&self.bucket_name)
                    .key(object.key.unwrap())
                    .send()
                    .await
                    .expect("Delete object");
            }
        }
        self.client
            .delete_bucket()
            .bucket(&self.bucket_name)
            .send()
            .await
            .expect("Delete bucket");
    }
}

impl Drop for TempBucket {
    fn drop(&mut self) {
        println!("Delete bucket {}", self.bucket_name);
        self.runtime.block_on(self.delete());
    }
}

fn conserve() -> Command {
    Command::cargo_bin("conserve").expect("locate conserve binary")
}

#[test]
fn integration_test() {
    let temp_bucket = TempBucket::new();
    let url = &temp_bucket.url().to_string();
    println!("init {url}");
    conserve().arg("init").arg(url).assert().success();

    // There are no versions in an empty archive
    println!("versions {url}");
    conserve()
        .arg("versions")
        .arg(url)
        .assert()
        .success()
        .stdout("")
        .stderr("");

    // An empty archive is valid
    println!("validate {url}");
    conserve().arg("validate").arg(url).assert().success();

    // Make a backup
    println!("backup {url}");
    conserve()
        .arg("backup")
        .arg(url)
        .arg("testdata/tree/minimal")
        .assert()
        .success();

    // There is one version
    println!("versions {url}");
    conserve()
        .arg("versions")
        .arg("--short")
        .arg(url)
        .assert()
        .success()
        .stdout("b0000\n")
        .stderr("");

    // It's valid
    println!("validate {url}");
    conserve().arg("validate").arg(url).assert().success();

    // Can list files in the backup
    println!("ls {url}");
    conserve()
        .arg("ls")
        .arg(url)
        .assert()
        .success()
        .stdout(indoc! { "
            /
            /hello
            /subdir
            /subdir/subfile
        "})
        .stderr("");

    let restore_dir = tempfile::tempdir().expect("Create tempdir");
    println!("restore {url}");
    conserve()
        .arg("restore")
        .arg(url)
        .arg(restore_dir.path())
        .assert()
        .success();
    // TODO: Compare contents

    println!("delete from {url}");
    conserve()
        .arg("delete")
        .arg(url)
        .arg("-b")
        .arg("b0000")
        .assert()
        .success();

    println!("validate {url}");
    conserve().arg("validate").arg(url).assert().success();

    println!("gc {url}");
    conserve().arg("gc").arg(url).assert().success();

    println!("gc {url}");
    conserve().arg("gc").arg(url).assert().success();

    println!("versions {url}");
    conserve()
        .arg("versions")
        .arg("--short")
        .arg(url)
        .assert()
        .success()
        .stdout("")
        .stderr("");
}
