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

//! Tests focused on backup behavior.

use std::sync::Arc;

use assert_fs::prelude::*;
use assert_fs::TempDir;
use filetime::{set_file_mtime, FileTime};
use tracing_test::traced_test;

use conserve::counters::Counter;
use conserve::monitor::test::TestMonitor;
use conserve::test_fixtures::TreeFixture;
use conserve::*;

const HELLO_HASH: &str =
    "9063990e5c5b2184877f92adace7c801a549b00c39cd7549877f06d5dd0d3a6ca6eee42d5\
     896bdac64831c8114c55cee664078bd105dc691270c92644ccb2ce7";

#[tokio::test]
async fn simple_backup() -> Result<()> {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    srcdir.create_file("hello");

    let monitor = TestMonitor::arc();
    let backup_stats = backup(
        &af,
        srcdir.path(),
        &BackupOptions::default(),
        monitor.clone(),
    )
    .await
    .expect("backup");
    assert_eq!(monitor.get_counter(Counter::IndexWrites), 1);
    assert_eq!(backup_stats.files, 1);
    assert_eq!(backup_stats.deduplicated_blocks, 0);
    assert_eq!(backup_stats.written_blocks, 1);
    assert_eq!(backup_stats.uncompressed_bytes, 8);
    assert_eq!(backup_stats.compressed_bytes, 10);
    check_backup(&af).await?;

    let restore_dir = TempDir::new().unwrap();

    let archive = Archive::open(af.transport().clone()).await.unwrap();
    assert!(archive.band_exists(BandId::zero()).await.unwrap());
    assert!(archive.band_is_closed(BandId::zero()).await.unwrap());
    assert!(!archive.band_exists(BandId::new(&[1])).await.unwrap());
    restore(
        &archive,
        restore_dir.path(),
        RestoreOptions::default(),
        monitor.clone(),
    )
    .await
    .expect("restore");

    monitor.assert_counter(Counter::FileBytes, 8);
    Ok(())
}

#[tokio::test]
#[traced_test]
async fn simple_backup_with_excludes() -> Result<()> {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    srcdir.create_file("hello");
    srcdir.create_file("foooo");
    srcdir.create_file("bar");
    srcdir.create_file("baz");
    // TODO: Include a symlink only on Unix.
    let exclude = Exclude::from_strings(["/**/baz", "/**/bar", "/**/fooo*"]).unwrap();
    let options = BackupOptions {
        exclude,
        ..BackupOptions::default()
    };
    let monitor = TestMonitor::arc();
    let stats = backup(&af, srcdir.path(), &options, monitor.clone())
        .await
        .expect("backup");

    check_backup(&af).await?;

    let counters = monitor.counters();
    dbg!(counters);
    assert_eq!(monitor.get_counter(Counter::IndexWrites), 1);
    assert_eq!(stats.files, 1);
    // TODO: Check stats for the number of excluded entries.
    assert!(counters.get(Counter::IndexWriteCompressedBytes) > 100);
    assert!(counters.get(Counter::IndexWriteUncompressedBytes) > 200);

    let restore_dir = TempDir::new().unwrap();

    let archive = Archive::open(af.transport().clone()).await.unwrap();

    let band = Band::open(&archive, BandId::zero()).await.unwrap();
    let band_info = band.get_info().await?;
    assert_eq!(band_info.index_hunk_count, Some(1));
    assert_eq!(band_info.id, BandId::zero());
    assert!(band_info.is_closed);
    assert!(band_info.end_time.is_some());

    let monitor = TestMonitor::arc();
    restore(
        &archive,
        restore_dir.path(),
        RestoreOptions::default(),
        monitor.clone(),
    )
    .await
    .expect("restore");
    monitor.assert_counter(Counter::FileBytes, 8);
    // TODO: Read back contents of that file.
    // TODO: Check index stats.
    // TODO: Check what was restored.

    af.validate(&ValidateOptions::default(), Arc::new(TestMonitor::new()))
        .await
        .unwrap();
    assert!(!logs_contain("ERROR") && !logs_contain("WARN"));
    Ok(())
}

#[tokio::test]
async fn backup_more_excludes() {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();

    srcdir.create_dir("test");
    srcdir.create_dir("foooooo");
    srcdir.create_file("foo");
    srcdir.create_file("fooBar");
    srcdir.create_file("foooooo/test");
    srcdir.create_file("test/baz");
    srcdir.create_file("baz");
    srcdir.create_file("bar");

    let exclude = Exclude::from_strings(["/**/foo*", "/**/baz"]).unwrap();
    let options = BackupOptions {
        exclude,
        ..Default::default()
    };
    let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
        .await
        .expect("backup");

    assert_eq!(1, stats.written_blocks);
    assert_eq!(1, stats.files);
    assert_eq!(1, stats.new_files);
    assert_eq!(2, stats.directories);
    assert_eq!(0, stats.symlinks);
    assert_eq!(0, stats.unknown_kind);
}

async fn check_backup(archive: &Archive) -> Result<()> {
    let band_ids = archive.list_band_ids().await.unwrap();
    assert_eq!(1, band_ids.len());
    assert_eq!("b0000", band_ids[0].to_string());
    assert_eq!(
        archive.last_complete_band().await.unwrap().unwrap().id(),
        BandId::new(&[0])
    );

    let band = Band::open(archive, band_ids[0]).await.unwrap();
    assert!(band.is_closed().await.unwrap());

    let index_entries = band
        .index()
        .iter_available_hunks()
        .await
        .collect_entry_vec()
        .await?;
    assert_eq!(2, index_entries.len());

    let root_entry = &index_entries[0];
    assert_eq!("/", root_entry.apath.to_string());
    assert_eq!(Kind::Dir, root_entry.kind);
    assert!(root_entry.mtime > 0);

    let file_entry = &index_entries[1];
    assert_eq!("/hello", file_entry.apath.to_string());
    assert_eq!(Kind::File, file_entry.kind);
    assert!(file_entry.mtime > 0);

    assert_eq!(
        archive
            .referenced_blocks(&archive.list_band_ids().await.unwrap(), TestMonitor::arc())
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.to_string())
            .collect::<Vec<String>>(),
        vec![HELLO_HASH]
    );
    assert_eq!(
        archive
            .all_blocks(TestMonitor::arc())
            .await
            .unwrap()
            .into_iter()
            .map(|h| h.to_string())
            .collect::<Vec<String>>(),
        vec![HELLO_HASH]
    );
    assert_eq!(
        archive
            .unreferenced_blocks(TestMonitor::arc())
            .await
            .unwrap()
            .len(),
        0
    );
    Ok(())
}

#[tokio::test]
async fn large_file() {
    let af = Archive::create_temp().await;
    let tf = TreeFixture::new();

    let file_size = 4 << 20;
    let large_content = vec![b'a'; file_size];
    tf.create_file_with_contents("large", &large_content);

    let monitor = TestMonitor::arc();
    let backup_stats = backup(
        &af,
        tf.path(),
        &BackupOptions {
            max_block_size: 1 << 20,
            ..Default::default()
        },
        monitor.clone(),
    )
    .await
    .expect("backup");
    assert_eq!(backup_stats.new_files, 1);
    // First 1MB should be new; remainder should be deduplicated.
    assert_eq!(backup_stats.uncompressed_bytes, 1 << 20);
    assert_eq!(backup_stats.written_blocks, 1);
    assert_eq!(backup_stats.deduplicated_blocks, 3);
    assert_eq!(backup_stats.deduplicated_bytes, 3 << 20);
    assert_eq!(backup_stats.errors, 0);
    assert_eq!(monitor.get_counter(Counter::IndexWrites), 1);

    // Try to restore it
    let rd = TempDir::new().unwrap();
    let restore_archive = Archive::open(af.transport().clone()).await.unwrap();
    let monitor = TestMonitor::arc();
    restore(
        &restore_archive,
        rd.path(),
        RestoreOptions::default(),
        monitor.clone(),
    )
    .await
    .expect("restore");
    monitor.assert_no_errors();
    monitor.assert_counter(Counter::Files, 1);
    monitor.assert_counter(Counter::FileBytes, file_size);

    let content = std::fs::read(rd.path().join("large")).unwrap();
    assert_eq!(large_content, content);
}

/// If some files are unreadable, others are stored and the backup completes with warnings.
#[cfg(unix)]
#[tokio::test]
async fn source_unreadable() {
    let af = Archive::create_temp().await;
    let tf = TreeFixture::new();

    tf.create_file("a");
    tf.create_file("b_unreadable");
    tf.create_file("c");

    tf.make_file_unreadable("b_unreadable");

    let stats = backup(
        &af,
        tf.path(),
        &BackupOptions::default(),
        TestMonitor::arc(),
    )
    .await
    .expect("backup");
    assert_eq!(stats.errors, 1);
    assert_eq!(stats.new_files, 3);
    assert_eq!(stats.files, 3);

    // TODO: On Windows change the ACL to make the file unreadable to the current user or to
    // everyone.
}

/// Files from before the Unix epoch can be backed up.
///
/// Reproduction of <https://github.com/sourcefrog/conserve/issues/100>.
#[tokio::test]
async fn mtime_before_epoch() {
    let tf = TreeFixture::new();
    let file_path = tf.create_file("old_file");

    let t1969 = FileTime::from_unix_time(-36_000, 0);
    set_file_mtime(file_path, t1969).expect("Failed to set file times");

    let lt = SourceTree::open(tf.path()).unwrap();
    let monitor = TestMonitor::arc();
    let entries = lt
        .iter_entries(Apath::root(), Exclude::nothing(), monitor.clone())
        .unwrap()
        .collect::<Vec<_>>();

    assert_eq!(entries[0].apath(), "/");
    assert_eq!(entries[1].apath(), "/old_file");

    let af = Archive::create_temp().await;
    backup(
        &af,
        tf.path(),
        &BackupOptions::default(),
        TestMonitor::arc(),
    )
    .await
    .expect("backup shouldn't crash on before-epoch mtimes");
}

#[cfg(unix)]
#[tokio::test]
async fn symlink() -> Result<()> {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    srcdir.create_symlink("symlink", "/a/broken/destination");

    let copy_stats = backup(
        &af,
        srcdir.path(),
        &BackupOptions::default(),
        TestMonitor::arc(),
    )
    .await
    .expect("backup");

    assert_eq!(0, copy_stats.files);
    assert_eq!(1, copy_stats.symlinks);
    assert_eq!(0, copy_stats.unknown_kind);

    let band_ids = af.list_band_ids().await.unwrap();
    assert_eq!(1, band_ids.len());
    assert_eq!("b0000", band_ids[0].to_string());

    let band = Band::open(&af, band_ids[0]).await.unwrap();
    assert!(band.is_closed().await.unwrap());

    let index_entries = band
        .index()
        .iter_available_hunks()
        .await
        .collect_entry_vec()
        .await?;
    assert_eq!(2, index_entries.len());

    let e2 = &index_entries[1];
    assert_eq!(e2.kind(), Kind::Symlink);
    assert_eq!(&e2.apath, "/symlink");
    assert_eq!(e2.target.as_ref().unwrap(), "/a/broken/destination");
    Ok(())
}

#[tokio::test]
async fn empty_file_uses_zero_blocks() {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    srcdir.create_file_with_contents("empty", &[]);
    let stats = backup(
        &af,
        srcdir.path(),
        &BackupOptions::default(),
        TestMonitor::arc(),
    )
    .await
    .unwrap();

    assert_eq!(1, stats.files);
    assert_eq!(stats.written_blocks, 0);

    // Read back the empty file
    let st = af
        .open_stored_tree(BandSelectionPolicy::Latest)
        .await
        .unwrap();
    let entries = st
        .iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
        .collect_all()
        .await
        .unwrap();
    let empty_entry = entries
        .iter()
        .find(|i| &i.apath == "/empty")
        .expect("found one entry");
    assert_eq!(empty_entry.addrs, []);

    // Restore it
    let dest = TempDir::new().unwrap();
    restore(
        &af,
        dest.path(),
        RestoreOptions::default(),
        TestMonitor::arc(),
    )
    .await
    .expect("restore");
    // TODO: Check restore stats.
    dest.child("empty").assert("");
}

#[tokio::test]
async fn detect_unmodified() {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    srcdir.create_file("aaa");
    srcdir.create_file("bbb");

    let options = BackupOptions::default();
    let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
        .await
        .unwrap();

    assert_eq!(stats.files, 2);
    assert_eq!(stats.new_files, 2);
    assert_eq!(stats.unmodified_files, 0);

    // Make a second backup from the same tree, and we should see that
    // both files are unmodified.
    let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
        .await
        .unwrap();

    assert_eq!(stats.files, 2);
    assert_eq!(stats.new_files, 0);
    assert_eq!(stats.unmodified_files, 2);

    // Change one of the files, and in a new backup it should be recognized
    // as unmodified.
    srcdir.create_file_with_contents("bbb", b"longer content for bbb");

    let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
        .await
        .unwrap();

    assert_eq!(stats.files, 2);
    assert_eq!(stats.new_files, 0);
    assert_eq!(stats.unmodified_files, 1);
    assert_eq!(stats.modified_files, 1);
}

#[tokio::test]
async fn detect_minimal_mtime_change() {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    srcdir.create_file("aaa");
    srcdir.create_file_with_contents("bbb", b"longer content for bbb");

    let options = BackupOptions::default();
    let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
        .await
        .unwrap();

    assert_eq!(stats.files, 2);
    assert_eq!(stats.new_files, 2);
    assert_eq!(stats.unmodified_files, 0);
    assert_eq!(stats.modified_files, 0);

    // Spin until the file's mtime is visibly different to what it was before.
    let bpath = srcdir.path().join("bbb");
    let orig_mtime = std::fs::metadata(&bpath).unwrap().modified().unwrap();
    loop {
        // Sleep a little while, so that even on systems with less than
        // nanosecond filesystem time resolution we can still see this is later.
        std::thread::sleep(std::time::Duration::from_millis(50));
        // Change one of the files, keeping the same length. If the mtime
        // changed, even fractionally, we should see the file was changed.
        srcdir.create_file_with_contents("bbb", b"woofer content for bbb");
        if std::fs::metadata(&bpath).unwrap().modified().unwrap() != orig_mtime {
            break;
        }
    }

    let stats = backup(&af, srcdir.path(), &options, TestMonitor::arc())
        .await
        .unwrap();
    assert_eq!(stats.files, 2);
    assert_eq!(stats.unmodified_files, 1);
}

#[tokio::test]
async fn small_files_combined_two_backups() {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    srcdir.create_file("file1");
    srcdir.create_file("file2");

    let stats1 = backup(
        &af,
        srcdir.path(),
        &BackupOptions::default(),
        TestMonitor::arc(),
    )
    .await
    .unwrap();
    // Although the two files have the same content, we do not yet dedupe them
    // within a combined block, so the block is different to when one identical
    // file is stored alone. This could be fixed.
    assert_eq!(stats1.combined_blocks, 1);
    assert_eq!(stats1.new_files, 2);
    assert_eq!(stats1.written_blocks, 1);
    assert_eq!(stats1.new_files, 2);

    // Add one more file, also identical, but it is not combined with the previous blocks.
    // This is a shortcoming of the current dedupe approach.
    srcdir.create_file("file3");
    let stats2 = backup(
        &af,
        srcdir.path(),
        &BackupOptions::default(),
        TestMonitor::arc(),
    )
    .await
    .unwrap();
    assert_eq!(stats2.new_files, 1);
    assert_eq!(stats2.unmodified_files, 2);
    assert_eq!(stats2.written_blocks, 1);
    assert_eq!(stats2.combined_blocks, 1);

    assert_eq!(af.all_blocks(TestMonitor::arc()).await.unwrap().len(), 2);
}

#[tokio::test]
async fn many_small_files_combined_to_one_block() {
    // tracing_subscriber::fmt::init();
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    // The directory also counts as an entry, so we should be able to fit 1999
    // files in 2 hunks of 1000 entries.
    for i in 0..1999 {
        srcdir.create_file_of_length_with_prefix(
            &format!("file{i:04}"),
            200,
            format!("something about {i}").as_bytes(),
        );
    }
    let backup_options = BackupOptions {
        max_entries_per_hunk: 1000,
        ..Default::default()
    };
    let monitor = TestMonitor::arc();
    let stats = backup(&af, srcdir.path(), &backup_options, monitor.clone())
        .await
        .expect("backup");
    assert_eq!(
        monitor.get_counter(Counter::IndexWrites),
        2,
        "expect exactly 2 hunks"
    );
    assert_eq!(stats.files, 1999);
    assert_eq!(stats.directories, 1);
    assert_eq!(stats.unknown_kind, 0);

    assert_eq!(stats.new_files, 1999);
    assert_eq!(stats.small_combined_files, 1999);
    assert_eq!(stats.errors, 0);
    // We write two combined blocks
    assert_eq!(stats.written_blocks, 2);
    assert_eq!(stats.combined_blocks, 2);

    let tree = af
        .open_stored_tree(BandSelectionPolicy::Latest)
        .await
        .unwrap();
    let entries = tree
        .iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
        .collect_all()
        .await
        .unwrap();
    assert_eq!(entries[0].apath(), "/");
    for (i, entry) in entries.iter().skip(1).enumerate() {
        assert_eq!(entry.apath().to_string(), format!("/file{i:04}"));
    }
    assert_eq!(entries.len(), 2000);
}

#[tokio::test]
async fn mixed_medium_small_files_two_hunks() {
    // tracing_subscriber::fmt::init();

    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    const MEDIUM_LENGTH: u64 = 150_000;
    // Make some files large enough not to be grouped together as small files.
    for i in 0..1999 {
        let name = format!("file{i:04}");
        if i % 100 == 0 {
            srcdir.create_file_of_length_with_prefix(&name, MEDIUM_LENGTH, b"something");
        } else {
            srcdir.create_file(&name);
        }
    }
    let backup_options = BackupOptions {
        max_entries_per_hunk: 1000,
        small_file_cap: 100_000,
        ..Default::default()
    };
    let monitor = TestMonitor::arc();
    let stats = backup(&af, srcdir.path(), &backup_options, monitor.clone())
        .await
        .expect("backup");
    assert_eq!(
        monitor.get_counter(Counter::IndexWrites),
        2,
        "expect exactly 2 hunks"
    );
    assert_eq!(stats.files, 1999);
    assert_eq!(stats.directories, 1);
    assert_eq!(stats.unknown_kind, 0);

    assert_eq!(stats.new_files, 1999);
    assert_eq!(stats.single_block_files, 20);
    assert_eq!(stats.small_combined_files, 1999 - 20);
    assert_eq!(stats.errors, 0);
    // There's one deduped block for all the large files, and then one per hunk for all the small combined files.
    assert_eq!(stats.written_blocks, 3);

    let tree = af
        .open_stored_tree(BandSelectionPolicy::Latest)
        .await
        .unwrap();
    let entries = tree
        .iter_entries(Apath::root(), Exclude::nothing(), TestMonitor::arc())
        .collect_all()
        .await
        .unwrap();
    assert_eq!(entries[0].apath(), "/");
    for (i, entry) in entries.iter().skip(1).enumerate() {
        assert_eq!(entry.apath().to_string(), format!("/file{i:04}"));
    }
    assert_eq!(entries.len(), 2000);
}

#[tokio::test]
async fn detect_unchanged_from_stitched_index() {
    let af = Archive::create_temp().await;
    let srcdir = TreeFixture::new();
    srcdir.create_file("a");
    srcdir.create_file("b");
    // Use small hunks for easier manipulation.
    let monitor = TestMonitor::arc();
    let stats = backup(
        &af,
        srcdir.path(),
        &BackupOptions {
            max_entries_per_hunk: 1,
            ..Default::default()
        },
        monitor.clone(),
    )
    .await
    .unwrap();
    assert_eq!(stats.new_files, 2);
    assert_eq!(stats.small_combined_files, 2);
    assert_eq!(monitor.get_counter(Counter::IndexWrites), 3,);

    // Make a second backup, with the first file changed.
    let monitor = TestMonitor::arc();
    srcdir.create_file_with_contents("a", b"new a contents");
    let stats = backup(
        &af,
        srcdir.path(),
        &BackupOptions {
            max_entries_per_hunk: 1,
            ..Default::default()
        },
        monitor.clone(),
    )
    .await
    .unwrap();
    assert_eq!(stats.unmodified_files, 1);
    assert_eq!(stats.modified_files, 1);
    assert_eq!(monitor.get_counter(Counter::IndexWrites), 3,);

    // Delete the last hunk and reopen the last band.
    af.transport().remove_file("b0001/BANDTAIL").await.unwrap();
    af.transport()
        .remove_file("b0001/i/00000/000000002")
        .await
        .unwrap();

    // The third backup should see nothing changed, by looking at the stitched
    // index from both b0 and b1.
    let monitor = TestMonitor::arc();
    let stats = backup(
        &af,
        srcdir.path(),
        &BackupOptions {
            max_entries_per_hunk: 1,
            ..Default::default()
        },
        monitor.clone(),
    )
    .await
    .unwrap();
    assert_eq!(stats.unmodified_files, 2, "both files are unmodified");
    assert_eq!(monitor.get_counter(Counter::IndexWrites), 3);
}
