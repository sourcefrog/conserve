// Conserve backup system.
// Copyright 2017, 2018, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! A `DeleteGuard` prevents block deletion while either a backup is pending,
//! or if a band is created concurrently with garbage enumeration, or if
//! another gc operation is underway.
//!
//! Deletion of blocks works by: finding all the blocks that are present,
//! then finding all the blocks that are referenced, then deleting the
//! blocks that are present but unreferenced.
//!
//! This matches the order in which data is written to the archive: data
//! blocks first, and then the index hunks that reference them.
//!
//! However, if a backup was running concurrently with garbage collection,
//! it's possible that we'd see the block and read the index before the
//! backup gets around to writing the index.
//!
//! Therefore, before starting enumeration, we check the latest band id,
//! and if it exists it must be complete. Then, after finding the blocks to
//! delete but before starting to actually delete them, we check that no
//! new bands have been created.

use tracing::{error, trace};
use transport::WriteMode;

use crate::*;

pub static GC_LOCK: &str = "GC_LOCK";

#[derive(Debug)]
pub struct GarbageCollectionLock {
    archive: Archive,

    /// Last band id present when the guard was created. May be None if
    /// there are no bands.
    band_id: Option<BandId>,
}

/// Lock on an archive for gc, that excludes backups and gc by other processes.
///
/// The lock is released when the object is dropped.
impl GarbageCollectionLock {
    /// Lock this archive for garbage collection.
    ///
    /// Returns `Err(Error::DeleteWithIncompleteBackup)` if the last
    /// backup is incomplete.
    pub async fn new(archive: &Archive) -> Result<GarbageCollectionLock> {
        let archive = archive.clone();
        let band_id = archive.last_band_id().await?;
        if let Some(band_id) = band_id {
            if !archive.band_is_closed(band_id).await? {
                return Err(Error::DeleteWithIncompleteBackup { band_id });
            }
        }
        if archive.transport().is_file(GC_LOCK).await.unwrap_or(true) {
            return Err(Error::GarbageCollectionLockHeld);
        }
        archive
            .transport()
            .write(GC_LOCK, b"{}\n", WriteMode::CreateNew)
            .await?;
        Ok(GarbageCollectionLock { archive, band_id })
    }

    /// Take a lock on an archive, breaking any existing gc lock.
    ///
    /// Use this only if you're confident that the process owning the lock
    /// has terminated and the lock is stale.
    pub async fn break_lock(archive: &Archive) -> Result<GarbageCollectionLock> {
        if GarbageCollectionLock::is_locked(archive).await? {
            archive.transport().remove_file(GC_LOCK).await?;
        }
        GarbageCollectionLock::new(archive).await
    }

    /// Returns true if the archive is currently locked by a gc process.
    pub async fn is_locked(archive: &Archive) -> Result<bool> {
        archive
            .transport()
            .is_file(GC_LOCK)
            .await
            .map_err(Error::from)
    }

    /// Check that no new versions have been created in this archive since
    /// the guard was created.
    pub async fn check(&self) -> Result<()> {
        let current_last_band_id = self.archive.last_band_id().await?;
        if self.band_id == current_last_band_id {
            Ok(())
        } else {
            Err(Error::GarbageCollectionLockHeldDuringBackup)
        }
    }

    /// Explicitly release the lock.
    ///
    /// Awaiting the future will ensure that the lock is released.
    pub async fn release(self) -> Result<()> {
        trace!("Releasing GC lock");
        self.archive
            .transport()
            .remove_file(GC_LOCK)
            .await
            .map_err(|err| {
                error!(?err, "Failed to delete GC lock");
                Error::from(err)
            })
    }
}

impl Drop for GarbageCollectionLock {
    fn drop(&mut self) {
        // The lock will, hopefully, be deleted soon after the lock is dropped,
        // and before the process exits.
        let transport = self.archive.transport().clone();
        tokio::task::spawn(async move {
            transport.remove_file(GC_LOCK).await.inspect_err(|err| {
                // Print directly to stderr, in case the UI structure is in a
                // bad state during unwind.
                eprintln!("Failed to delete GC_LOCK from Drop: {err:?}");
            })
        });
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;
    use crate::monitor::test::TestMonitor;
    use crate::test_fixtures::TreeFixture;

    #[tokio::test]
    async fn empty_archive_ok() {
        let archive = Archive::create_temp().await;
        let delete_guard = GarbageCollectionLock::new(&archive).await.unwrap();
        assert!(archive.transport().is_file("GC_LOCK").await.unwrap());
        delete_guard.check().await.unwrap();

        // Released when dropped.
        drop(delete_guard);
        // Cleanup is async: hard to know exactly when it will complete, but this should be
        // sufficient?
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!archive.transport().is_file("GC_LOCK").await.unwrap());
    }

    #[tokio::test]
    async fn completed_backup_ok() {
        let archive = Archive::create_temp().await;
        let source = TreeFixture::new();
        backup(
            &archive,
            source.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await
        .unwrap();
        let delete_guard = GarbageCollectionLock::new(&archive).await.unwrap();
        delete_guard.check().await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_complete_backup_denied() {
        let archive = Archive::create_temp().await;
        let source = TreeFixture::new();
        let _delete_guard = GarbageCollectionLock::new(&archive).await.unwrap();
        let backup_result = backup(
            &archive,
            source.path(),
            &BackupOptions::default(),
            TestMonitor::arc(),
        )
        .await;
        assert_eq!(
            backup_result.expect_err("backup fails").to_string(),
            "Archive is locked for garbage collection"
        );
    }

    #[tokio::test]
    async fn incomplete_backup_denied() {
        let archive = Archive::create_temp().await;
        Band::create(&archive).await.unwrap();
        let err = GarbageCollectionLock::new(&archive).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "Can't delete blocks because the last band (b0000) is incomplete and may be in use"
        );
    }

    #[tokio::test]
    async fn concurrent_gc_prevented() {
        let archive = Archive::create_temp().await;
        let _lock1 = GarbageCollectionLock::new(&archive).await.unwrap();
        // Should not be able to create a second lock while one gc is running.
        let lock2_result = GarbageCollectionLock::new(&archive).await;
        assert_eq!(
            lock2_result.unwrap_err().to_string(),
            "Archive is locked for garbage collection"
        );
    }

    #[tokio::test]
    async fn sequential_gc_allowed() {
        let archive = Archive::create_temp().await;
        let lock1 = GarbageCollectionLock::new(&archive).await.unwrap();
        lock1.release().await.unwrap();
        let lock2 = GarbageCollectionLock::new(&archive).await.unwrap();
        lock2.release().await.unwrap();
    }

    #[tokio::test]
    async fn break_lock() {
        let archive = Archive::create_temp().await;
        let lock1 = GarbageCollectionLock::new(&archive).await.unwrap();
        // Pretend the process owning lock1 died, and get a new lock.
        std::mem::forget(lock1);
        let _lock2 = GarbageCollectionLock::break_lock(&archive).await.unwrap();
    }
}
