// Copyright 2015-2025 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Test deletion.

use std::time::Duration;

use conserve::monitor::test::TestMonitor;
use conserve::test_fixtures::store_two_versions;
use conserve::*;
use tempfile::TempDir;
use time::OffsetDateTime;

#[tokio::test]
async fn delete_all_bands() {
    let af = Archive::create_temp().await;
    store_two_versions(&af).await;

    let stats = af
        .delete_bands(
            &[BandId::new(&[0]), BandId::new(&[1])],
            &Default::default(),
            TestMonitor::arc(),
        )
        .await
        .expect("delete_bands");

    assert_eq!(stats.deleted_block_count, 2);
    assert_eq!(stats.deleted_band_count, 2);
}

#[tokio::test]
async fn delete_expired() -> Result<()> {
    let af = Archive::create_temp().await;
    let srcdir = TempDir::new().unwrap();
    let monitor = TestMonitor::arc();
    let now = OffsetDateTime::now_utc();

    let day = Duration::from_secs(24 * 3600);
    let mut backup_options = BackupOptions {
        override_start_time: Some(now - 20 * day),
        ..Default::default()
    };
    let _stats = backup(&af, srcdir.path(), &backup_options, monitor.clone()).await?;

    backup_options.override_start_time = Some(now - 10 * day);
    let _stats = backup(&af, srcdir.path(), &backup_options, monitor.clone()).await?;

    backup_options.override_start_time = Some(now - day);
    let _stats = backup(&af, srcdir.path(), &backup_options, monitor.clone()).await?;

    let delete_stats = af
        .delete_bands(
            &[],
            &DeleteOptions {
                expiry_days: Some(2),
                ..Default::default()
            },
            monitor.clone(),
        )
        .await
        .expect("delete_bands");

    dbg!(&delete_stats);
    assert_eq!(delete_stats.deleted_block_count, 0);
    assert_eq!(delete_stats.deleted_band_count, 2);
    Ok(())
}
