// Copyright 2015, 2016, 2017 Martin Pool.

/// Test Conserve through its public API.
extern crate conserve;

extern crate tempdir;

use std::fs::File;
use std::io::prelude::*;

use conserve::test_fixtures::ScratchArchive;
use conserve::test_fixtures::TreeFixture;
use conserve::*;

const HELLO_HASH: &'static str =
    "9063990e5c5b2184877f92adace7c801a549b00c39cd7549877f06d5dd0d3a6ca6eee42d5\
     896bdac64831c8114c55cee664078bd105dc691270c92644ccb2ce7";

#[test]
pub fn simple_backup() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file("hello");
    // TODO: Include a symlink only on Unix.
    copy_tree(&srcdir.live_tree(), &mut BackupWriter::begin(&af).unwrap()).unwrap();
    check_backup(&af, &af.report());
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
    let mut bw = BackupWriter::begin(&af).unwrap();
    copy_tree(&lt, &mut bw).unwrap();
    check_backup(&af, &af.report());
    check_restore(&af);
    af.validate().unwrap();
}

fn check_backup(af: &ScratchArchive, report: &Report) {
    assert_eq!(1, report.get_count("block.write"));
    assert_eq!(1, report.get_count("file"));
    assert_eq!(1, report.get_count("dir"));
    assert_eq!(0, report.get_count("skipped.unsupported_file_kind"));

    let band_ids = af.list_bands().unwrap();
    assert_eq!(1, band_ids.len());
    assert_eq!("b0000", band_ids[0].to_string());
    assert_eq!(af.last_complete_band().unwrap().id(), BandId::new(&[0]));

    let dur = report.get_duration("source.read");
    let read_us = (dur.subsec_nanos() as u64) / 1000u64 + dur.as_secs() * 1000000u64;
    assert!(read_us > 0);

    let band = Band::open(&af, &band_ids[0]).unwrap();
    assert!(band.is_closed().unwrap());

    let index_entries = band
        .index_iter(&excludes::excludes_nothing(), &report)
        .unwrap()
        .filter_map(|i| i.ok())
        .collect::<Vec<IndexEntry>>();
    assert_eq!(2, index_entries.len());

    let root_entry = &index_entries[0];
    assert_eq!("/", root_entry.apath);
    assert_eq!(Kind::Dir, root_entry.kind);
    assert!(root_entry.mtime.unwrap() > 0);

    let file_entry = &index_entries[1];
    assert_eq!("/hello", file_entry.apath);
    assert_eq!(Kind::File, file_entry.kind);
    assert!(file_entry.mtime.unwrap() > 0);
    let hash = file_entry.blake2b.as_ref().unwrap();
    assert_eq!(HELLO_HASH, hash);

    assert_eq!(
        af.referenced_blocks()
            .unwrap()
            .into_iter()
            .collect::<Vec<String>>(),
        vec![HELLO_HASH]
    );
    assert_eq!(
        af.block_dir().blocks(&af.report()).unwrap(),
        vec![HELLO_HASH]
    );
}

fn check_restore(af: &ScratchArchive) {
    // TODO: Read back contents of that file.
    let restore_dir = TreeFixture::new();

    let restore_report = Report::new();
    let restore_a = Archive::open(af.path(), &restore_report).unwrap();
    let mut restore_tree = RestoreTree::create(&restore_dir.path(), &restore_report).unwrap();
    let st = StoredTree::open_last(&restore_a).unwrap();
    copy_tree(&st, &mut restore_tree).unwrap();

    let block_sizes = restore_report.get_size("block");
    assert!(
        block_sizes.uncompressed == 8 && block_sizes.compressed == 10,
        format!("{:?}", block_sizes)
    );
    let index_sizes = restore_report.get_size("index");
    assert_eq!(
        index_sizes.uncompressed, 462,
        "index_sizes.uncompressed on restore"
    );
    assert!(index_sizes.compressed <= 292, index_sizes.compressed);
    // TODO: Check what was restored.
}

/// Store and retrieve large files.
#[test]
fn large_file() {
    let af = ScratchArchive::new();

    let tf = TreeFixture::new();
    let large_content = String::from("a sample large file\n").repeat(1000000);
    tf.create_file_with_contents("large", &large_content.as_bytes());
    let report = af.report();
    let mut bw = BackupWriter::begin(&af).unwrap();
    copy_tree(&tf.live_tree(), &mut bw).unwrap();
    assert_eq!(report.get_count("file"), 1);
    assert_eq!(report.get_count("file.large"), 1);

    // Try to restore it
    let rd = tempdir::TempDir::new("conserve_test_restore").unwrap();
    let restore_report = Report::new();
    let restore_archive = Archive::open(af.path(), &restore_report).unwrap();
    let st = StoredTree::open_last(&restore_archive).unwrap();
    let mut rt = RestoreTree::create(rd.path(), &restore_report).unwrap();
    copy_tree(&st, &mut rt).unwrap();

    assert_eq!(report.get_count("file"), 1);

    // TODO: Restore should also set file.large etc.
    let mut content = String::new();
    File::open(rd.path().join("large"))
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    assert_eq!(large_content, content);
}
