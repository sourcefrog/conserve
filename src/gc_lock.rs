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
//! or if a band is created concurrently with garbage enumeration.
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

pub(crate) struct GarbageCollectionLock {
    /// Last band id present when the guard was created. May be None if
    /// there are no bands.
    band_id: Option<BandId>,

    archive: Archive,
}

impl GarbageCollectionLock {
    /// Create a soft lock on this archive.
    ///
    /// Returns `Err(Error::DeleteWithIncompleteBackup)` if the last
    /// backup is incomplete.
    pub fn new(archive: &Archive) -> Result<GarbageCollectionLock> {
        let archive = archive.clone();
        if let Some(band_id) = archive.last_band_id()? {
            if archive.band_is_closed(&band_id)? {
                Ok(GarbageCollectionLock {
                    archive,
                    band_id: Some(band_id),
                })
            } else {
                Err(Error::DeleteWithIncompleteBackup { band_id })
            }
        } else {
            Ok(GarbageCollectionLock {
                archive,
                band_id: None,
            })
        }
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_fixtures::{ScratchArchive, TreeFixture};

    #[test]
    fn empty_archive_ok() {
        let archive = ScratchArchive::new();
        let delete_guard = GarbageCollectionLock::new(&archive).unwrap();
        delete_guard.check().unwrap();
    }

    #[test]
    fn completed_backup_ok() {
        let archive = ScratchArchive::new();
        let source = TreeFixture::new();
        archive
            .backup(&source.path(), &BackupOptions::default())
            .unwrap();
        let delete_guard = GarbageCollectionLock::new(&archive).unwrap();
        delete_guard.check().unwrap();
    }

    #[test]
    fn concurrent_complete_backup_denied() {
        let archive = ScratchArchive::new();
        let source = TreeFixture::new();
        let delete_guard = GarbageCollectionLock::new(&archive).unwrap();
        archive
            .backup(&source.path(), &BackupOptions::default())
            .unwrap();
        let result = delete_guard.check();
        assert_eq!(
            result.err().expect("guard check fails").to_string(),
            "Can't continue with deletion because the archive was changed by another process"
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
}
