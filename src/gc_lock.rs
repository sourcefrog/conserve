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

use crate::*;

const GC_LOCK: &str = "GC_LOCK";

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
    pub fn new(archive: &Archive) -> Result<GarbageCollectionLock> {
        let archive = archive.clone();
        let band_id = archive.last_band_id()?;
        if let Some(band_id) = band_id.clone() {
            if !archive.band_is_closed(&band_id)? {
                return Err(Error::DeleteWithIncompleteBackup { band_id });
            }
        }
        if archive.transport().is_file(GC_LOCK).unwrap_or(true) {
            return Err(Error::GarbageCollectionLockHeld {});
        }
        archive.transport().write_file(GC_LOCK, b"{}\n")?;
        Ok(GarbageCollectionLock { archive, band_id })
    }

    /// Take a lock on an archive, breaking any existing gc lock.
    ///
    /// Use this only if you're confident that the process owning the lock
    /// has terminated and the lock is stale.
    pub fn break_lock(archive: &Archive) -> Result<GarbageCollectionLock> {
        if GarbageCollectionLock::is_locked(archive)? {
            archive.transport().remove_file(GC_LOCK)?;
        }
        GarbageCollectionLock::new(archive)
    }

    /// Returns true if the archive is currently locked by a gc process.
    pub fn is_locked(archive: &Archive) -> Result<bool> {
        archive.transport().is_file(GC_LOCK).map_err(Error::from)
    }

    /// Check that no new versions have been created in this archive since
    /// the guard was created.
    pub fn check(&self) -> Result<()> {
        let current_last_band_id = self.archive.last_band_id()?;
        if self.band_id == current_last_band_id {
            Ok(())
        } else {
            Err(Error::DeleteWithConcurrentActivity)
        }
    }
}

impl Drop for GarbageCollectionLock {
    fn drop(&mut self) {
        if let Err(err) = self.archive.transport().remove_file(GC_LOCK) {
            // Print directly to stderr, in case the UI structure is in a
            // bad state during unwind.
            eprintln!("Failed to delete GC_LOCK: {err:?}")
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_fixtures::{ScratchArchive, TreeFixture};

    #[test]
    fn empty_archive_ok() {
        let archive = ScratchArchive::new();
        let delete_guard = GarbageCollectionLock::new(&archive).unwrap();
        assert!(archive.transport().is_file("GC_LOCK").unwrap());
        delete_guard.check().unwrap();

        // Released when dropped.
        drop(delete_guard);
        assert!(!archive.transport().is_file("GC_LOCK").unwrap());
    }

    #[test]
    fn completed_backup_ok() {
        let archive = ScratchArchive::new();
        let source = TreeFixture::new();
        backup(
            &archive,
            &source.live_tree(),
            &BackupOptions::default(),
            None,
        )
        .unwrap();
        let delete_guard = GarbageCollectionLock::new(&archive).unwrap();
        delete_guard.check().unwrap();
    }

    #[test]
    fn concurrent_complete_backup_denied() {
        let archive = ScratchArchive::new();
        let source = TreeFixture::new();
        let _delete_guard = GarbageCollectionLock::new(&archive).unwrap();
        let backup_result = backup(
            &archive,
            &source.live_tree(),
            &BackupOptions::default(),
            None,
        );
        assert_eq!(
            backup_result.expect_err("backup fails").to_string(),
            "Archive is locked for garbage collection"
        );
    }

    #[test]
    fn incomplete_backup_denied() {
        let archive = ScratchArchive::new();
        Band::create(&archive).unwrap();
        let result = GarbageCollectionLock::new(&archive);
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().to_string(),
            "Can't delete blocks because the last band (b0000) is incomplete and may be in use"
        );
    }

    #[test]
    fn concurrent_gc_prevented() {
        let archive = ScratchArchive::new();
        let _lock1 = GarbageCollectionLock::new(&archive).unwrap();
        // Should not be able to create a second lock while one gc is running.
        let lock2_result = GarbageCollectionLock::new(&archive);
        match lock2_result {
            Err(Error::GarbageCollectionLockHeld) => (),
            other => panic!("unexpected result {other:?}"),
        };
    }

    #[test]
    fn sequential_gc_allowed() {
        let archive = ScratchArchive::new();
        let _lock1 = GarbageCollectionLock::new(&archive).unwrap();
        drop(_lock1);
        let _lock2 = GarbageCollectionLock::new(&archive).unwrap();
        drop(_lock2);
    }

    #[test]
    fn break_lock() {
        let archive = ScratchArchive::new();
        let lock1 = GarbageCollectionLock::new(&archive).unwrap();
        // Pretend the process owning lock1 died, and get a new lock.
        std::mem::forget(lock1);
        let _lock2 = GarbageCollectionLock::break_lock(&archive).unwrap();
    }
}
