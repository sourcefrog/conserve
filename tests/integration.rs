// Copyright 2015, 2016, 2017, 2019, 2020 Martin Pool.

/// Test Conserve through its public API.
use std::fs::File;
use std::io::prelude::*;

use tempfile::TempDir;

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
    let copy_stats = copy_tree(
        &srcdir.live_tree(),
        BackupWriter::begin(&af).unwrap(),
        &COPY_DEFAULT,
    )
    .unwrap();
    assert_eq!(copy_stats.index_builder_stats.index_hunks, 1);
    assert_eq!(copy_stats.files, 1);
    check_backup(&af);
    check_restore(&af);
}

#[test]
pub fn simple_backup_with_excludes() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file("hello");
    srcdir.create_file("foooo");
    srcdir.create_file("bar");
    srcdir.create_file("baz");
    // TODO: Include a symlink only on Unix.
    let excludes = excludes::from_strings(&["/**/baz", "/**/bar", "/**/fooo*"]).unwrap();
    let lt = srcdir.live_tree().with_excludes(excludes);
    let bw = BackupWriter::begin(&af).unwrap();
    let copy_stats = copy_tree(&lt, bw, &COPY_DEFAULT).unwrap();
    check_backup(&af);

    assert_eq!(copy_stats.index_builder_stats.index_hunks, 1);
    assert_eq!(copy_stats.files, 1);
    // TODO: Check stats for the number of excluded entries.
    assert!(copy_stats.index_builder_stats.compressed_index_bytes > 100);
    assert!(copy_stats.index_builder_stats.uncompressed_index_bytes > 200);

    check_restore(&af);
    af.validate().unwrap();
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

    let index_entries = band.iter_entries().unwrap().collect::<Vec<IndexEntry>>();
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
            .collect::<Vec<String>>(),
        vec![HELLO_HASH]
    );
    assert_eq!(
        af.block_dir()
            .block_names()
            .unwrap()
            .collect::<Vec<String>>(),
        vec![HELLO_HASH]
    );
}

fn check_restore(af: &ScratchArchive) {
    // TODO: Read back contents of that file.
    let restore_dir = TreeFixture::new();

    let archive = Archive::open_path(af.path()).unwrap();
    let restore_tree = RestoreTree::create(&restore_dir.path()).unwrap();
    let st = StoredTree::open_last(&archive).unwrap();
    let copy_stats = copy_tree(&st, restore_tree, &COPY_DEFAULT).unwrap();
    assert_eq!(copy_stats.uncompressed_bytes, 8);
    // TODO: Compressed size isn't set properly when restoring, because it's
    // lost by passing through a std::io::Read in ReadStoredFile.
    // TODO: Check index stats.
    // TODO: Check what was restored.
}

/// Store and retrieve large files.
#[test]
fn large_file() {
    let af = ScratchArchive::new();

    let tf = TreeFixture::new();
    let large_content = String::from("a sample large file\n").repeat(1_000_000);
    tf.create_file_with_contents("large", &large_content.as_bytes());
    let bw = BackupWriter::begin(&af).unwrap();
    let _stats = copy_tree(&tf.live_tree(), bw, &COPY_DEFAULT).unwrap();
    // TODO: Examine stats from copy_tree.

    // Try to restore it
    let rd = TempDir::new().unwrap();
    let restore_archive = Archive::open_path(af.path()).unwrap();
    let st = StoredTree::open_last(&restore_archive).unwrap();
    let rt = RestoreTree::create(rd.path().to_owned()).unwrap();
    let _stats = copy_tree(&st, rt, &COPY_DEFAULT).unwrap();
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

    let bw = BackupWriter::begin(&af).unwrap();
    let r = copy_tree(&tf.live_tree(), bw, &COPY_DEFAULT);
    r.unwrap();

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

    utime::set_file_times(&file_path, -36000, -36000).expect("Failed to set file times");

    let lt = LiveTree::open(tf.path()).unwrap();
    let entries = lt.iter_entries().unwrap().collect::<Vec<_>>();

    assert_eq!(entries[0].apath(), "/");
    assert_eq!(entries[1].apath(), "/old_file");
    dbg!(&entries[1].mtime());

    let af = ScratchArchive::new();
    let bw = BackupWriter::begin(&af).unwrap();
    let _copy_stats = copy_tree(&lt, bw, &COPY_DEFAULT).unwrap();
}
