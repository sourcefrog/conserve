// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Diff two trees: for example a live tree against a stored tree.
//!
//! See also [conserve::show_diff] to format the diff as text.

use std::sync::Arc;

use crate::monitor::Monitor;
use crate::*;

#[derive(Debug)]
pub struct DiffOptions {
    pub exclude: Exclude,
    pub include_unchanged: bool,
    // TODO: An option to filter to a subtree?
    // TODO: Optionally compare all the content?
}

impl Default for DiffOptions {
    fn default() -> Self {
        DiffOptions {
            exclude: Exclude::nothing(),
            include_unchanged: false,
        }
    }
}

/// An async pseudo-iterator that yields a series of changes between two trees.
// TODO: This is barely any different to Merge, maybe we should just merge them?
// But, it does look a bit more at the contents of the entry, rather than just
// aligning by apath.
pub struct Diff {
    merge: MergeTrees,
    options: DiffOptions,
    // monitor: Arc<dyn Monitor>,
}

/// Generate an iter of per-entry diffs between two trees.
pub async fn diff(
    st: &StoredTree,
    lt: &SourceTree,
    options: DiffOptions,
    monitor: Arc<dyn Monitor>,
) -> Result<Diff> {
    let a = st.iter_entries(Apath::root(), options.exclude.clone(), monitor.clone());
    let b = lt.iter_entries(Apath::root(), options.exclude.clone(), monitor.clone())?;
    let merge = MergeTrees::new(a, b);
    Ok(Diff {
        merge,
        options,
        // monitor,
    })
}

impl Diff {
    pub async fn next(&mut self) -> Option<EntryChange> {
        while let Some(merge_entry) = self.merge.next().await {
            let ec = merge_entry.to_entry_change();
            if self.options.include_unchanged || !ec.change.is_unchanged() {
                return Some(ec);
            }
        }
        None
    }

    /// Collect all the diff entries.
    ///
    /// This is a convenience method for testing and small trees.
    pub async fn collect(&mut self) -> Vec<EntryChange> {
        let mut changes = Vec::new();
        while let Some(change) = self.next().await {
            changes.push(change);
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use filetime::{set_file_mtime, FileTime};

    use crate::monitor::test::TestMonitor;
    use crate::test_fixtures::TreeFixture;
    use crate::*;

    /// Make a tree with one file and an archive with one version.
    async fn create_tree() -> (Archive, TreeFixture) {
        let a = Archive::create_temp().await;
        let tf = TreeFixture::new();
        tf.create_file_with_contents("thing", b"contents of thing");
        let stats = backup(&a, tf.path(), &BackupOptions::default(), TestMonitor::arc())
            .await
            .unwrap();
        assert_eq!(stats.new_files, 1);
        (a, tf)
    }

    #[tokio::test]
    async fn diff_unchanged() {
        let (a, tf) = create_tree().await;

        let st = a
            .open_stored_tree(BandSelectionPolicy::Latest)
            .await
            .unwrap();

        let options = DiffOptions {
            include_unchanged: true,
            ..DiffOptions::default()
        };
        let monitor = TestMonitor::arc();
        let changes: Vec<EntryChange> = diff(&st, &tf.live_tree(), options, monitor.clone())
            .await
            .unwrap()
            .collect()
            .await;
        dbg!(&changes);
        assert_eq!(changes.len(), 2); // Root directory and the file "/thing".
        assert_eq!(changes[0].apath, "/");
        assert!(changes[0].change.is_unchanged());
        assert!(!changes[0].change.is_changed());
        assert_eq!(changes[1].apath, "/thing");
        assert!(changes[1].change.is_unchanged());
        assert!(!changes[1].change.is_changed());

        // Excluding unchanged elements
        let options = DiffOptions {
            include_unchanged: false,
            ..DiffOptions::default()
        };
        let changes = diff(&st, &tf.live_tree(), options, TestMonitor::arc())
            .await
            .unwrap()
            .collect()
            .await;
        println!("changes with include_unchanged=false:\n{changes:#?}");
        assert_eq!(changes.len(), 0);
    }

    #[tokio::test]
    async fn mtime_only_change_reported_as_changed() {
        let (a, tf) = create_tree().await;

        let st = a
            .open_stored_tree(BandSelectionPolicy::Latest)
            .await
            .unwrap();
        set_file_mtime(
            tf.path().join("thing"),
            FileTime::from_unix_time(1704135090, 0),
        )
        .unwrap();

        let options = DiffOptions {
            include_unchanged: false,
            ..DiffOptions::default()
        };
        let changes: Vec<EntryChange> = diff(&st, &tf.live_tree(), options, TestMonitor::arc())
            .await
            .unwrap()
            .collect()
            .await;
        dbg!(&changes);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].apath, "/thing");
        assert!(changes[0].change.is_changed());
        assert!(!changes[0].change.is_unchanged());
    }

    // Test only on Linux, as macOS doesn't seem to have a way to get all groups
    // (see https://docs.rs/nix/latest/nix/unistd/fn.getgroups.html).
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn chgrp_reported_as_changed() {
        use std::os::unix::fs::chown;

        use crate::test_fixtures::arbitrary_secondary_group; // only on Linux
        let Some(secondary_group) = arbitrary_secondary_group() else {
            // maybe running on a machine where the user has only one group
            return;
        };

        let (a, tf) = create_tree().await;

        chown(tf.path().join("thing"), None, Some(secondary_group)).unwrap();
        let st = a
            .open_stored_tree(BandSelectionPolicy::Latest)
            .await
            .unwrap();

        let options = DiffOptions {
            include_unchanged: false,
            ..DiffOptions::default()
        };
        let changes: Vec<EntryChange> = diff(&st, &tf.live_tree(), options, TestMonitor::arc())
            .await
            .unwrap()
            .collect()
            .await;
        dbg!(&changes);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].apath, "/thing");
        assert!(changes[0].change.is_changed());
        assert!(!changes[0].change.is_unchanged());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_target_change_reported_as_changed() {
        use std::fs::remove_file;
        use std::path::Path;

        let a = Archive::create_temp().await;
        let tf = TreeFixture::new();
        tf.create_symlink("link", "target");
        backup(&a, tf.path(), &BackupOptions::default(), TestMonitor::arc())
            .await
            .unwrap();

        let link_path = tf.path().join("link");
        remove_file(&link_path).unwrap();
        std::os::unix::fs::symlink("new-target", &link_path).unwrap();
        let st = a
            .open_stored_tree(BandSelectionPolicy::Latest)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_link(&link_path).unwrap(),
            Path::new("new-target")
        );

        let options = DiffOptions {
            include_unchanged: false,
            ..DiffOptions::default()
        };
        let changes: Vec<EntryChange> = diff(&st, &tf.live_tree(), options, TestMonitor::arc())
            .await
            .unwrap()
            .collect()
            .await;
        dbg!(&changes);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].apath, "/link");
        assert!(changes[0].change.is_changed());
        assert!(!changes[0].change.is_unchanged());
    }
}
