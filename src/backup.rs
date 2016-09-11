// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::fs;
use std::path::{Path};
use std::time;

use super::apath;
use super::archive::Archive;
use super::block::{BlockDir};
use super::errors::*;
use super::index;
use super::index::{IndexBuilder, IndexKind};
use super::report::Report;
use super::sources;


struct Backup {
    block_dir: BlockDir,
    index_builder: IndexBuilder,
    report: Report,
}


pub fn run_backup(archive_path: &Path, source: &Path, mut report: &mut Report) -> Result<()> {
    let archive = try!(Archive::open(archive_path));
    let band = try!(archive.create_band(&mut report));
    let mut backup = Backup {
        block_dir: band.block_dir(),
        index_builder: band.index_builder(),
        report: Report::new(),
    };
    let source_iter = try!(sources::iter(source));
    for entry in source_iter {
        try!(backup_one_source_entry(&mut backup, &try!(entry)));
    }
    try!(backup.index_builder.finish_hunk(report));
    try!(band.close(&mut backup.report));
    report.merge_from(&backup.report);
    Ok(())
}


fn backup_one_source_entry(backup: &mut Backup, entry: &sources::Entry) -> Result<()> {
    info!("backup {}", entry.path.display());
    assert!(apath::valid(&entry.apath), "invalid apath: {:?}", &entry.apath);
    let new_entry: index::Entry = if entry.metadata.is_file() {
        try!(backup_one_file(backup, entry))
    } else if entry.metadata.is_dir() {
        try!(backup_one_dir(backup, entry))
    } else {
        // TODO: Backup directories, symlinks, etc.
        warn!("Skipping unsupported file kind {}", &entry.apath);
        backup.report.increment("backup.skipped.unsupported_file_kind", 1);
        return Ok(())
    };
    backup.index_builder.push(new_entry);
    try!(backup.index_builder.maybe_flush(&mut backup.report));
    Ok(())
}


fn backup_one_dir(backup: &mut Backup, entry: &sources::Entry) -> Result<index::Entry> {
    backup.report.increment("backup.dir.count", 1);

    let mtime = entry.metadata.modified().ok()
        .and_then(|t| t.duration_since(time::UNIX_EPOCH).ok())
        .and_then(|dur| Some(dur.as_secs()));
    Ok(index::Entry {
        apath: entry.apath.clone(),
        mtime: mtime,
        kind: IndexKind::Dir,
        addrs: vec![],
        blake2b: None,
    })
}


fn backup_one_file(backup: &mut Backup, entry: &sources::Entry) -> Result<(index::Entry)> {
    backup.report.increment("backup.file.count", 1);

    let mut f = try!(fs::File::open(&entry.path));
    let (addrs, body_hash) = try!(backup.block_dir.store_file(&mut f, &mut backup.report));
    drop(f);

    let mtime = entry.metadata.modified().ok()
        .and_then(|t| t.duration_since(time::UNIX_EPOCH).ok())
        .and_then(|dur| Some(dur.as_secs()));
    Ok(index::Entry {
        apath: entry.apath.clone(),
        mtime: mtime,
        kind: IndexKind::File,
        blake2b: Some(body_hash),
        addrs: addrs,
    })
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::run_backup;
    use super::super::index;
    use super::super::report::Report;
    use super::super::testfixtures::{ScratchArchive};
    use conserve_testsupport::TreeFixture;

    #[test]
    pub fn simple_backup() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_file("hello");
        let mut report = Report::new();
        run_backup(af.path(), srcdir.path(), &mut report).unwrap();
        assert_eq!(1, report.get_count("block.write.count"));
        assert_eq!(1, report.get_count("backup.file.count"));
        assert_eq!(1, report.get_count("backup.dir.count"));
        assert_eq!(0, report.get_count("backup.skipped.unsupported_file_kind"));

        let band_ids = af.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].as_string());

        let dur = report.get_duration("source.read");
        let read_us = (dur.subsec_nanos() as u64) / 1000u64 + dur.as_secs() * 1000000u64;
        assert!(read_us > 0);

        let band = af.open_band(&band_ids[0], &mut report).unwrap();
        assert!(band.is_closed().unwrap());

        let index_entries = band.index_iter().unwrap()
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
    }

    #[cfg(unix)]
    #[test]
    pub fn symlink() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_symlink("symlink", "/a/broken/destination");
        let mut report = Report::new();
        run_backup(af.path(), srcdir.path(), &mut report).unwrap();
        assert_eq!(0, report.get_count("block.write.count"));
        assert_eq!(0, report.get_count("backup.file.count"));
        // TODO: Actually store the symlink.
        assert_eq!(1, report.get_count("backup.skipped.unsupported_file_kind"));

        let band_ids = af.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].as_string());

        // TODO: Once implemented  check the symlink is included.
    }
}
