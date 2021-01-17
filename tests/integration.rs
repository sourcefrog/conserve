// Copyright 2015, 2016, 2017, 2019, 2020 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Test Conserve through its public API.

use std::fs::File;
use std::io::prelude::*;

use assert_fs::prelude::*;
use assert_fs::TempDir;
use filetime::{set_file_mtime, FileTime};

use conserve::kind::Kind;
use conserve::test_fixtures::ScratchArchive;
use conserve::test_fixtures::TreeFixture;
use conserve::*;

const HELLO_HASH: &str =
    "9063990e5c5b2184877f92adace7c801a549b00c39cd7549877f06d5dd0d3a6ca6eee42d5\
     896bdac64831c8114c55cee664078bd105dc691270c92644ccb2ce7";

#[test]
pub fn simple_backup() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file("hello");
    // TODO: Include a symlink only on Unix.
    let copy_stats = backup(&af, &srcdir.live_tree(), &BackupOptions::default()).expect("backup");
    assert_eq!(copy_stats.index_builder_stats.index_hunks, 1);
    assert_eq!(copy_stats.files, 1);
    check_backup(&af);

    let restore_dir = TempDir::new().unwrap();

    let archive = Archive::open_path(af.path()).unwrap();
    assert_eq!(archive.band_exists(&BandId::zero()).unwrap(), true);
    assert_eq!(archive.band_is_closed(&BandId::zero()).unwrap(), true);
    assert_eq!(archive.band_exists(&BandId::new(&[1])).unwrap(), false);
    let copy_stats =
        restore(&archive, &restore_dir.path(), &RestoreOptions::default()).expect("restore");

    assert_eq!(copy_stats.uncompressed_bytes, 8);
}

#[test]
pub fn simple_backup_with_excludes() -> Result<()> {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file("hello");
    srcdir.create_file("foooo");
    srcdir.create_file("bar");
    srcdir.create_file("baz");
    // TODO: Include a symlink only on Unix.
    let excludes = excludes::from_strings(&["/**/baz", "/**/bar", "/**/fooo*"]).unwrap();
    let source = srcdir.live_tree();
    let options = BackupOptions {
        excludes,
        ..BackupOptions::default()
    };
    let copy_stats = backup(&af, &source, &options).expect("backup");

    check_backup(&af);

    assert_eq!(copy_stats.index_builder_stats.index_hunks, 1);
    assert_eq!(copy_stats.files, 1);
    // TODO: Check stats for the number of excluded entries.
    assert!(copy_stats.index_builder_stats.compressed_index_bytes > 100);
    assert!(copy_stats.index_builder_stats.uncompressed_index_bytes > 200);

    let restore_dir = TempDir::new().unwrap();

    let archive = Archive::open_path(af.path()).unwrap();

    let band = Band::open(&archive, &BandId::zero()).unwrap();
    let band_info = band.get_info()?;
    assert_eq!(band_info.index_hunk_count, Some(1));
    assert_eq!(band_info.id, BandId::zero());
    assert_eq!(band_info.is_closed, true);
    assert!(band_info.end_time.is_some());

    let copy_stats =
        restore(&archive, &restore_dir.path(), &RestoreOptions::default()).expect("restore");

    assert_eq!(copy_stats.uncompressed_bytes, 8);
    // TODO: Read back contents of that file.
    // TODO: Compressed size isn't set properly when restoring, because it's
    // lost by passing through a std::io::Read in ReadStoredFile.
    // TODO: Check index stats.
    // TODO: Check what was restored.

    let validate_stats = af.validate().unwrap();
    assert!(!validate_stats.has_problems());
    Ok(())
}

#[test]
pub fn backup_more_excludes() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();

    srcdir.create_dir("test");
    srcdir.create_dir("foooooo");
    srcdir.create_file("foo");
    srcdir.create_file("fooBar");
    srcdir.create_file("foooooo/test");
    srcdir.create_file("test/baz");
    srcdir.create_file("baz");
    srcdir.create_file("bar");

    let excludes = excludes::from_strings(&["/**/foo*", "/**/baz"]).unwrap();
    let source = srcdir.live_tree();
    let options = BackupOptions {
        excludes,
        print_filenames: false,
        ..Default::default()
    };
    let stats = backup(&af, &source, &options).expect("backup");

    assert_eq!(1, stats.written_blocks);
    assert_eq!(1, stats.files);
    assert_eq!(1, stats.new_files);
    assert_eq!(2, stats.directories);
    assert_eq!(0, stats.symlinks);
    assert_eq!(0, stats.unknown_kind);
}

fn check_backup(af: &ScratchArchive) {
    let band_ids = af.list_band_ids().unwrap();
    assert_eq!(1, band_ids.len());
    assert_eq!("b0000", band_ids[0].to_string());
    assert_eq!(
        *af.last_complete_band().unwrap().unwrap().id(),
        BandId::new(&[0])
    );

    let band = Band::open(&af, &band_ids[0]).unwrap();
    assert!(band.is_closed().unwrap());

    let index_entries = band.iter_entries().collect::<Vec<IndexEntry>>();
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
        af.referenced_blocks()
            .unwrap()
            .into_iter()
            .map(|h| h.to_string())
            .collect::<Vec<String>>(),
        vec![HELLO_HASH]
    );
    assert_eq!(
        af.block_dir()
            .block_names()
            .unwrap()
            .map(|h| h.to_string())
            .collect::<Vec<String>>(),
        vec![HELLO_HASH]
    );
    assert_eq!(af.unreferenced_blocks().unwrap().count(), 0);
}

/// Store and retrieve large files.
#[test]
fn large_file() {
    let af = ScratchArchive::new();

    let tf = TreeFixture::new();
    let large_content = String::from("abcd").repeat(1 << 20);
    tf.create_file_with_contents("large", &large_content.as_bytes());
    let copy_stats = backup(&af, &tf.live_tree(), &BackupOptions::default()).expect("backup");
    assert_eq!(copy_stats.new_files, 1);
    // First 1MB should be new; remainder should be deduplicated.
    assert_eq!(copy_stats.uncompressed_bytes, 1 << 20);
    assert_eq!(copy_stats.written_blocks, 1);
    assert_eq!(copy_stats.deduplicated_bytes, 3 << 20);
    assert_eq!(copy_stats.deduplicated_blocks, 3);
    assert_eq!(copy_stats.errors, 0);
    assert_eq!(copy_stats.index_builder_stats.index_hunks, 1);

    // Try to restore it
    let rd = TempDir::new().unwrap();
    let restore_archive = Archive::open_path(af.path()).unwrap();
    let _stats = restore(&restore_archive, rd.path(), &RestoreOptions::default()).expect("restore");
    // TODO: Examine stats.

    let mut content = String::new();
    File::open(rd.path().join("large"))
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    assert_eq!(large_content, content);
}

/// If some files are unreadable, others are stored and the backup completes with warnings.
#[cfg(unix)]
#[test]
fn source_unreadable() {
    let af = ScratchArchive::new();
    let tf = TreeFixture::new();

    tf.create_file("a");
    tf.create_file("b_unreadable");
    tf.create_file("c");

    tf.make_file_unreadable("b_unreadable");

    let stats = backup(&af, &tf.live_tree(), &BackupOptions::default()).expect("backup");
    assert_eq!(stats.errors, 1);
    assert_eq!(stats.new_files, 3);
    assert_eq!(stats.files, 3);

    // TODO: On Windows change the ACL to make the file unreadable to the current user or to
    // everyone.
}

/// Files from before the Unix epoch can be backed up.
///
/// Reproduction of <https://github.com/sourcefrog/conserve/issues/100>.
#[test]
fn mtime_before_epoch() {
    let tf = TreeFixture::new();
    let file_path = tf.create_file("old_file");

    let t1969 = FileTime::from_unix_time(-36_000, 0);
    set_file_mtime(&file_path, t1969).expect("Failed to set file times");

    let lt = LiveTree::open(tf.path()).unwrap();
    let entries = lt.iter_entries().unwrap().collect::<Vec<_>>();

    assert_eq!(entries[0].apath(), "/");
    assert_eq!(entries[1].apath(), "/old_file");
    dbg!(&entries[1].mtime());

    let af = ScratchArchive::new();
    backup(&af, &tf.live_tree(), &BackupOptions::default())
        .expect("backup shouldn't crash on before-epoch mtimes");
}

#[cfg(unix)]
#[test]
pub fn symlink() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_symlink("symlink", "/a/broken/destination");
    let copy_stats = backup(&af, &srcdir.live_tree(), &BackupOptions::default()).expect("backup");

    assert_eq!(0, copy_stats.files);
    assert_eq!(1, copy_stats.symlinks);
    assert_eq!(0, copy_stats.unknown_kind);

    let band_ids = af.list_band_ids().unwrap();
    assert_eq!(1, band_ids.len());
    assert_eq!("b0000", band_ids[0].to_string());

    let band = Band::open(&af, &band_ids[0]).unwrap();
    assert!(band.is_closed().unwrap());

    let index_entries = band.iter_entries().collect::<Vec<IndexEntry>>();
    assert_eq!(2, index_entries.len());

    let e2 = &index_entries[1];
    assert_eq!(e2.kind(), Kind::Symlink);
    assert_eq!(&e2.apath, "/symlink");
    assert_eq!(e2.target.as_ref().unwrap(), "/a/broken/destination");
}

#[test]
pub fn empty_file_uses_zero_blocks() {
    use std::io::Read;

    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file_with_contents("empty", &[]);
    let stats = backup(&af, &srcdir.live_tree(), &BackupOptions::default()).unwrap();

    assert_eq!(1, stats.files);
    assert_eq!(stats.written_blocks, 0);

    // Read back the empty file
    let st = af.open_stored_tree(BandSelectionPolicy::Latest).unwrap();
    let empty_entry = st
        .iter_entries()
        .unwrap()
        .find(|ref i| &i.apath == "/empty")
        .expect("found one entry");
    let mut sf = st.file_contents(&empty_entry).unwrap();
    let mut s = String::new();
    assert_eq!(sf.read_to_string(&mut s).unwrap(), 0);
    assert_eq!(s.len(), 0);

    // Restore it
    let dest = TempDir::new().unwrap();
    restore(&af, &dest.path(), &RestoreOptions::default()).expect("restore");
    // TODO: Check restore stats.
    dest.child("empty").assert("");
}

#[test]
pub fn detect_unmodified() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file("aaa");
    srcdir.create_file("bbb");

    let options = BackupOptions::default();
    let stats = backup(&af, &srcdir.live_tree(), &options).unwrap();

    assert_eq!(stats.files, 2);
    assert_eq!(stats.new_files, 2);
    assert_eq!(stats.unmodified_files, 0);

    // Make a second backup from the same tree, and we should see that
    // both files are unmodified.
    let stats = backup(&af, &srcdir.live_tree(), &options).unwrap();

    assert_eq!(stats.files, 2);
    assert_eq!(stats.new_files, 0);
    assert_eq!(stats.unmodified_files, 2);

    // Change one of the files, and in a new backup it should be recognized
    // as unmodified.
    srcdir.create_file_with_contents("bbb", b"longer content for bbb");

    let stats = backup(&af, &srcdir.live_tree(), &options).unwrap();

    assert_eq!(stats.files, 2);
    assert_eq!(stats.new_files, 0);
    assert_eq!(stats.unmodified_files, 1);
    assert_eq!(stats.modified_files, 1);
}

#[test]
pub fn detect_minimal_mtime_change() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file("aaa");
    srcdir.create_file_with_contents("bbb", b"longer content for bbb");

    let options = BackupOptions::default();
    let stats = backup(&af, &srcdir.live_tree(), &options).unwrap();

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

    let stats = backup(&af, &srcdir.live_tree(), &options).unwrap();
    assert_eq!(stats.files, 2);
    assert_eq!(stats.unmodified_files, 1);
}
