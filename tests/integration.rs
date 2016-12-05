/// Test Conserve through its public API.

extern crate tempdir;

extern crate conserve;
extern crate conserve_testsupport;

use conserve::backup;
use conserve::index;
use conserve::report::Report;
use conserve::restore;
use conserve::testfixtures::{ScratchArchive};
use conserve_testsupport::TreeFixture;

#[test]
pub fn simple_backup() {
    let af = ScratchArchive::new();
    let srcdir = TreeFixture::new();
    srcdir.create_file("hello");
    // TODO: Include a symlink only on Unix.
    let report = Report::new();
    backup(af.path(), srcdir.path(), &report).unwrap();
    {
        let cs = report.borrow_counts();
        assert_eq!(1, cs.get_count("block"));
        assert_eq!(1, cs.get_count("file"));
        assert_eq!(1, cs.get_count("dir"));
        assert_eq!(0, cs.get_count("skipped.unsupported_file_kind"));
    }

    let band_ids = af.list_bands().unwrap();
    assert_eq!(1, band_ids.len());
    assert_eq!("b0000", band_ids[0].as_string());

    let dur = report.borrow_counts().get_duration("source.read");
    let read_us = (dur.subsec_nanos() as u64) / 1000u64 + dur.as_secs() * 1000000u64;
    assert!(read_us > 0);

    let band = af.open_band(&band_ids[0], &report).unwrap();
    assert!(band.is_closed().unwrap());

    let index_entries = band.index_iter(&report).unwrap()
        .filter_map(|i| i.ok())
        .collect::<Vec<index::Entry>>();
    assert_eq!(2, index_entries.len());

    let root_entry = &index_entries[0];
    assert_eq!("/", root_entry.apath);
    assert_eq!(index::IndexKind::Dir, root_entry.kind);
    assert!(root_entry.mtime.unwrap() > 0);

    let file_entry = &index_entries[1];
    assert_eq!("/hello", file_entry.apath);
    assert_eq!(index::IndexKind::File, file_entry.kind);
    assert!(file_entry.mtime.unwrap() > 0);
    let hash = file_entry.blake2b.as_ref().unwrap();
    assert_eq!("9063990e5c5b2184877f92adace7c801a549b00c39cd7549877f06d5dd0d3a6ca6eee42d5896bdac64831c8114c55cee664078bd105dc691270c92644ccb2ce7",
        hash);

    // TODO: Read back contents of that file.
    let restore_dir = TreeFixture::new();
    let restore_report = Report::new();
    restore(af.path(), restore_dir.path(), &restore_report).unwrap();
    // TODO: Check what was restored.
}
