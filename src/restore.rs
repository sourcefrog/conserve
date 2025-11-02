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

//! Restore from the archive to the filesystem.

#[cfg(test)]
use std::collections::HashMap;
use std::fs::{File, create_dir_all};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use filetime::set_file_handle_times;
#[cfg(unix)]
use filetime::set_symlink_file_times;
use time::OffsetDateTime;
use tracing::{instrument, trace};

use crate::blockdir::BlockDir;
use crate::counters::Counter;
use crate::index::entry::IndexEntry;
use crate::io::{directory_is_empty, ensure_dir_exists};
use crate::monitor::Monitor;
use crate::unix_time::ToFileTime;
use crate::*;

/// Description of how to restore a tree.
pub struct RestoreOptions {
    /// Exclude these paths from being restored.
    pub exclude: Exclude,

    /// Restore only this subdirectory.
    pub only_subtree: Option<Apath>,

    /// Overwrite existing files in the destination.
    pub overwrite: bool,

    /// The band to select, or by default the last complete one.
    pub band_selection: BandSelectionPolicy,

    /// Call this callback as each entry is successfully restored.
    pub change_callback: Option<ChangeCallback>,

    /// For testing, fail to restore the named entries, with the given error.
    #[cfg(test)]
    pub inject_failures: HashMap<Apath, io::ErrorKind>,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        RestoreOptions {
            overwrite: false,
            band_selection: BandSelectionPolicy::LatestClosed,
            exclude: Exclude::nothing(),
            only_subtree: None,
            change_callback: None,
            #[cfg(test)]
            inject_failures: HashMap::new(),
        }
    }
}

/// Restore a selected version, or by default the latest, to a destination directory.
pub async fn restore(
    archive: &Archive,
    destination: &Path,
    options: RestoreOptions,
    monitor: Arc<dyn Monitor>,
) -> Result<()> {
    let st = archive
        .open_stored_tree(options.band_selection.clone())
        .await?;
    ensure_dir_exists(destination)?;
    if !options.overwrite && !directory_is_empty(destination)? {
        return Err(Error::DestinationNotEmpty);
    }
    let task = monitor.start_task("Restore".to_string());
    let block_dir = &archive.block_dir;
    // // This causes us to walk the source tree twice, which is probably an acceptable option
    // // since it's nice to see realistic overall progress. We could keep all the entries
    // // in memory, and maybe we should, but it might get unreasonably big.
    // if options.measure_first {
    //     progress_bar.set_phase("Measure source tree");
    //     // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
    //     // again a second time? But, that'll potentially use memory proportional to tree size, which
    //     // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
    //     // deleted or changed while this is running.
    //     progress_bar.set_bytes_total(st.size(options.excludes.clone())?.file_bytes as u64);
    // }
    let mut stitch = st.iter_entries(
        options.only_subtree.clone().unwrap_or_else(Apath::root),
        options.exclude.clone(),
        monitor.clone(),
    );
    let mut deferrals = Vec::new();
    while let Some(entry) = stitch.next().await {
        task.set_name(format!("Restore {}", entry.apath));
        let path = destination.join(&entry.apath[1..]);
        match entry.kind() {
            Kind::Dir => {
                monitor.count(Counter::Dirs, 1);
                if *entry.apath() != Apath::root() {
                    // Create all the parents in case we're restoring only a nested subtree.
                    if let Err(err) = restore_dir(&entry.apath, &path, &options) {
                        monitor.error(Error::RestoreDirectory {
                            path: path.clone(),
                            source: err,
                        });
                        continue;
                    }
                }
                deferrals.push(DirDeferral {
                    path,
                    unix_mode: entry.unix_mode(),
                    mtime: entry.mtime(),
                    owner: entry.owner().clone(),
                })
            }
            Kind::File => {
                monitor.count(Counter::Files, 1);
                if let Err(err) =
                    restore_file(path.clone(), &entry, block_dir, monitor.clone()).await
                {
                    monitor.error(err);
                    continue;
                }
            }
            Kind::Symlink => {
                monitor.count(Counter::Symlinks, 1);
                if let Err(err) = restore_symlink(&path, &entry) {
                    monitor.error(err);
                    continue;
                }
            }
            Kind::Unknown => {
                monitor.error(Error::InvalidMetadata {
                    details: format!("Unknown file kind {:?}", entry.apath()),
                });
            }
        };
        if let Some(cb) = options.change_callback.as_ref() {
            // Since we only restore to empty directories they're all added.
            cb(&EntryChange::added(&entry))?;
        }
    }
    apply_deferrals(&deferrals, monitor.clone())?;
    Ok(())
}

fn restore_dir(apath: &Apath, restore_path: &Path, options: &RestoreOptions) -> io::Result<()> {
    #[cfg(test)]
    {
        if let Some(err_kind) = options.inject_failures.get(apath) {
            return Err(io::Error::from(*err_kind));
        }
    }
    let _ = apath;
    let _ = options;
    create_dir_all(restore_path).or_else(|err| {
        if err.kind() == io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(err)
        }
    })
}

/// Recorded changes to apply to directories after all their contents
/// have been applied.
///
/// For example we might want to make the directory read-only, but we
/// shouldn't do that until we added all the children.
struct DirDeferral {
    path: PathBuf,
    unix_mode: UnixMode,
    mtime: OffsetDateTime,
    owner: Owner,
}

fn apply_deferrals(deferrals: &[DirDeferral], monitor: Arc<dyn Monitor>) -> Result<()> {
    for DirDeferral {
        path,
        unix_mode,
        mtime,
        owner,
    } in deferrals
    {
        if let Err(source) = owner.set_owner(path) {
            monitor.error(Error::RestoreOwnership {
                path: path.clone(),
                source,
            });
        }
        if let Err(source) = unix_mode.set_permissions(path) {
            monitor.error(Error::RestorePermissions {
                path: path.clone(),
                source,
            });
        }
        if let Err(source) = filetime::set_file_mtime(path, (*mtime).to_file_time()) {
            monitor.error(Error::RestoreModificationTime {
                path: path.clone(),
                source,
            });
        }
    }
    Ok(())
}

/// Copy in the contents of a file from another tree.
#[instrument(skip(source_entry, block_dir, monitor))]
async fn restore_file(
    path: PathBuf,
    source_entry: &IndexEntry,
    block_dir: &BlockDir,
    monitor: Arc<dyn Monitor>,
) -> Result<()> {
    let mut out = File::create(&path).map_err(|err| Error::RestoreFile {
        path: path.clone(),
        source: err,
    })?;
    for addr in &source_entry.addrs {
        // TODO: We could combine small parts
        // in memory, and then write them in a single system call. However
        // for the probably common cases of files with one part, or
        // many larger parts, sending everything through a BufWriter is
        // probably a waste.
        let bytes = block_dir
            .read_address(addr, monitor.clone())
            .await
            .map_err(|source| Error::RestoreFileBlock {
                apath: source_entry.apath.clone(),
                hash: addr.hash.clone(),
                source: Box::new(source),
            })?;
        out.write_all(&bytes).map_err(|err| Error::RestoreFile {
            path: path.clone(),
            source: err,
        })?;
        monitor.count(Counter::FileBytes, bytes.len());
    }
    out.flush().map_err(|source| Error::RestoreFile {
        path: path.clone(),
        source,
    })?;

    let mtime = Some(source_entry.mtime().to_file_time());
    set_file_handle_times(&out, mtime, mtime).map_err(|source| Error::RestoreModificationTime {
        path: path.clone(),
        source,
    })?;

    // Restore permissions only if there are mode bits stored in the archive
    if let Err(source) = source_entry.unix_mode().set_permissions(&path) {
        monitor.error(Error::RestorePermissions {
            path: path.clone(),
            source,
        });
    }

    // Restore ownership if possible.
    // TODO: Stats and warnings if a user or group is specified in the index but
    // does not exist on the local system.
    if let Err(source) = source_entry.owner().set_owner(&path) {
        monitor.error(Error::RestoreOwnership {
            path: path.clone(),
            source,
        });
    }
    // TODO: Accumulate more stats.
    trace!("Restored file");
    Ok(())
}

#[cfg(unix)]
fn restore_symlink(path: &Path, entry: &IndexEntry) -> Result<()> {
    use std::os::unix::fs as unix_fs;
    if let Some(ref target) = entry.symlink_target() {
        if let Err(source) = unix_fs::symlink(target, path) {
            return Err(Error::RestoreSymlink {
                path: path.to_owned(),
                source,
            });
        }
        if let Err(source) = entry.owner().set_owner(path) {
            return Err(Error::RestoreOwnership {
                path: path.to_owned(),
                source,
            });
        }
        let mtime = entry.mtime().to_file_time();
        if let Err(source) = set_symlink_file_times(path, mtime, mtime) {
            return Err(Error::RestoreModificationTime {
                path: path.to_owned(),
                source,
            });
        }
    } else {
        return Err(Error::InvalidMetadata {
            details: format!("No target in symlink entry {:?}", entry.apath()),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
#[mutants::skip]
fn restore_symlink(_restore_path: &Path, entry: &IndexEntry) -> Result<()> {
    // TODO: Add a test with a canned index containing a symlink, and expect
    // it cannot be restored on Windows and can be on Unix.
    tracing::warn!("Can't restore symlinks on non-Unix: {}", entry.apath());
    Ok(())
}

#[cfg(test)]
mod test {
    use std::fs::{create_dir, write};
    use std::io;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use tempfile::TempDir;

    use crate::counters::Counter;
    use crate::monitor::test::TestMonitor;
    use crate::test_fixtures::{TreeFixture, store_two_versions};
    use crate::transport::Transport;
    use crate::*;

    #[tokio::test]
    async fn simple_restore() {
        let af = Archive::create_temp().await;
        store_two_versions(&af).await;
        let destdir = TreeFixture::new();
        let restore_archive = Archive::open(af.transport().clone()).await.unwrap();
        let restored_names = Arc::new(Mutex::new(Vec::new()));
        let restored_names_clone = restored_names.clone();
        let options = RestoreOptions {
            change_callback: Some(Box::new(move |entry_change| {
                restored_names_clone
                    .lock()
                    .unwrap()
                    .push(entry_change.apath.clone());
                Ok(())
            })),
            ..Default::default()
        };
        let monitor = TestMonitor::arc();
        restore(&restore_archive, destdir.path(), options, monitor.clone())
            .await
            .expect("restore");

        monitor.assert_no_errors();
        monitor.assert_counter(Counter::Files, 3);
        let mut expected_names = vec![
            "/",
            "/hello",
            "/hello2",
            "/link",
            "/subdir",
            "/subdir/subfile",
        ];
        if !SYMLINKS_SUPPORTED {
            expected_names.retain(|n| *n != "/link");
        }
        assert_eq!(restored_names.lock().unwrap().as_slice(), expected_names);

        let dest = &destdir.path();
        assert!(dest.join("hello").is_file());
        assert!(dest.join("hello2").is_file());
        assert!(dest.join("subdir").is_dir());
        assert!(dest.join("subdir").join("subfile").is_file());
        if SYMLINKS_SUPPORTED {
            let dest = std::fs::read_link(dest.join("link")).unwrap();
            assert_eq!(dest.to_string_lossy(), "target");
        }

        // TODO: Test file contents are as expected.
    }

    #[tokio::test]
    async fn restore_specified_band() {
        let af = Archive::create_temp().await;
        store_two_versions(&af).await;
        let destdir = TreeFixture::new();
        let archive = Archive::open(af.transport().clone()).await.unwrap();
        let band_id = BandId::new(&[0]);
        let options = RestoreOptions {
            band_selection: BandSelectionPolicy::Specified(band_id),
            ..RestoreOptions::default()
        };
        let monitor = TestMonitor::arc();
        restore(&archive, destdir.path(), options, monitor.clone())
            .await
            .expect("restore");
        monitor.assert_no_errors();
        // Does not have the 'hello2' file added in the second version.
        monitor.assert_counter(Counter::Files, 2);
    }

    /// Restoring a subdirectory works, and restores the parent directories:
    ///
    /// <https://github.com/sourcefrog/conserve/issues/268>
    #[tokio::test]
    async fn restore_only_subdir() {
        // We need the selected directory to be more than one level down, because the bug was that
        // its parent was not created.
        let backup_monitor = TestMonitor::arc();
        let src = TempDir::new().unwrap();
        create_dir(src.path().join("parent")).unwrap();
        create_dir(src.path().join("parent/sub")).unwrap();
        write(src.path().join("parent/sub/file"), b"hello").unwrap();
        let af = Archive::create_temp().await;
        backup(
            &af,
            src.path(),
            &BackupOptions::default(),
            backup_monitor.clone(),
        )
        .await
        .unwrap();
        backup_monitor.assert_counter(Counter::Files, 1);
        backup_monitor.assert_no_errors();

        let destdir = TreeFixture::new();
        let restore_monitor = TestMonitor::arc();
        let archive = Archive::open(af.transport().clone()).await.unwrap();
        let options = RestoreOptions {
            only_subtree: Some(Apath::from("/parent/sub")),
            ..Default::default()
        };
        restore(&archive, destdir.path(), options, restore_monitor.clone())
            .await
            .expect("restore");
        restore_monitor.assert_no_errors();
        assert!(destdir.path().join("parent").is_dir());
        assert!(destdir.path().join("parent/sub/file").is_file());
        dbg!(restore_monitor.counters());
        restore_monitor.assert_counter(Counter::Files, 1);
    }

    #[tokio::test]
    async fn decline_to_overwrite() {
        let af = Archive::create_temp().await;
        store_two_versions(&af).await;
        let destdir = TreeFixture::new();
        destdir.create_file("existing");
        let options = RestoreOptions {
            ..RestoreOptions::default()
        };
        assert!(!options.overwrite, "overwrite is false by default");
        let restore_err_str = restore(&af, destdir.path(), options, TestMonitor::arc())
            .await
            .expect_err("restore should fail if the destination exists")
            .to_string();
        assert!(
            restore_err_str.contains("Destination directory is not empty"),
            "Unexpected error message: {restore_err_str:?}"
        );
    }

    #[tokio::test]
    async fn forced_overwrite() {
        let af = Archive::create_temp().await;
        store_two_versions(&af).await;
        let destdir = TreeFixture::new();
        destdir.create_file("existing");

        let restore_archive = Archive::open(af.transport().clone()).await.unwrap();
        let options = RestoreOptions {
            overwrite: true,
            ..RestoreOptions::default()
        };
        let monitor = TestMonitor::arc();
        restore(&restore_archive, destdir.path(), options, monitor.clone())
            .await
            .expect("restore");
        monitor.assert_no_errors();
        monitor.assert_counter(Counter::Files, 3);
        let dest = destdir.path();
        assert!(dest.join("hello").is_file());
        assert!(dest.join("existing").is_file());
    }

    #[tokio::test]
    async fn exclude_files() {
        let af = Archive::create_temp().await;
        store_two_versions(&af).await;
        let destdir = TreeFixture::new();
        let restore_archive = Archive::open(af.transport().clone()).await.unwrap();
        let options = RestoreOptions {
            overwrite: true,
            exclude: Exclude::from_strings(["/**/subfile"]).unwrap(),
            ..RestoreOptions::default()
        };
        let monitor = TestMonitor::arc();
        restore(&restore_archive, destdir.path(), options, monitor.clone())
            .await
            .expect("restore");

        let dest = destdir.path();
        assert!(dest.join("hello").is_file());
        assert!(dest.join("hello2").is_file());
        assert!(dest.join("subdir").is_dir());
        monitor.assert_no_errors();
        monitor.assert_counter(Counter::Files, 2);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn restore_symlink() {
        use std::fs::{read_link, symlink_metadata};
        use std::path::PathBuf;

        use filetime::{FileTime, set_symlink_file_times};

        let af = Archive::create_temp().await;
        let srcdir = TreeFixture::new();

        srcdir.create_symlink("symlink", "target");
        let years_ago = FileTime::from_unix_time(189216000, 0);
        set_symlink_file_times(srcdir.path().join("symlink"), years_ago, years_ago).unwrap();

        let monitor = TestMonitor::arc();
        backup(&af, srcdir.path(), &Default::default(), monitor.clone())
            .await
            .unwrap();

        let restore_dir = TempDir::new().unwrap();
        let monitor = TestMonitor::arc();
        restore(&af, restore_dir.path(), Default::default(), monitor.clone())
            .await
            .unwrap();

        let restored_symlink_path = restore_dir.path().join("symlink");
        let sym_meta = symlink_metadata(&restored_symlink_path).unwrap();
        assert!(sym_meta.file_type().is_symlink());
        assert_eq!(FileTime::from(sym_meta.modified().unwrap()), years_ago);
        assert_eq!(
            read_link(&restored_symlink_path).unwrap(),
            PathBuf::from("target")
        );
    }

    #[tokio::test]
    async fn create_dir_permission_denied() {
        let archive = Archive::open(Transport::local(Path::new(
            "testdata/archive/simple/v0.6.10",
        )))
        .await
        .unwrap();

        let mut restore_options = RestoreOptions::default();
        restore_options
            .inject_failures
            .insert(Apath::from("/subdir"), io::ErrorKind::PermissionDenied);
        let restore_tmp = TempDir::new().unwrap();
        let monitor = TestMonitor::arc();
        restore(
            &archive,
            restore_tmp.path(),
            restore_options,
            monitor.clone(),
        )
        .await
        .expect("Restore");
        let errors = monitor.take_errors();
        dbg!(&errors);
        assert_eq!(errors.len(), 2);
        if let Error::RestoreDirectory { path, .. } = &errors[0] {
            assert!(path.ends_with("subdir"));
        } else {
            panic!("Unexpected error {:?}", errors[0]);
        }
        // Also, since we didn't create the directory, we fail to create the file within it.
        if let Error::RestoreFile { path, source } = &errors[1] {
            assert!(path.ends_with("subdir/subfile"));
            assert_eq!(source.kind(), io::ErrorKind::NotFound);
        } else {
            panic!("Unexpected error {:?}", errors[1]);
        }
    }
}
