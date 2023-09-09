// Copyright 2023 Martin Pool

#![cfg(feature = "s3-integration-test")]

/// Test s3 transport, only when the `s3-integration-test`
/// feature is enabled.
///
/// This must be run with AWS credentials available, e.g. in
/// the environment.

#[test]
fn hello() {
    assert_eq!(1, 1);
}
