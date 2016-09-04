// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::fs;
use std::path::{Path};
use std::io;

use super::apath;
use super::archive::Archive;
use super::block::{BlockDir, BlockWriter};
use super::index::{IndexBuilder, IndexEntry, IndexKind};
use super::report::Report;
use super::sources;


struct Backup {
    block_dir: BlockDir,
    index_builder: IndexBuilder,
    report: Report,
}


pub fn run_backup(archive_path: &Path, source: &Path, mut report: &mut Report)
    -> io::Result<()> {
    // TODO: More tests.
    // TODO: Backup directories and symlinks too.

    let archive = try!(Archive::open(archive_path));
    let band = try!(archive.create_band(&mut report));
    let mut backup = Backup {
        block_dir: band.block_dir(),
        index_builder: band.index_builder(),
        report: Report::new(),
    };

    let source_iter = sources::iter(source);
    for entry in source_iter {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                // TODO: Optionally continue?
                warn!("{}", e);
                return Err(e);
            }
        };
        try!(backup_one_source_entry(&mut backup, &entry));
    }
    try!(backup.index_builder.finish_hunk(report));
    try!(band.close(&mut backup.report));
    report.merge_from(&backup.report);
    Ok(())
}

fn backup_one_source_entry(backup: &mut Backup, entry: &sources::Entry) -> io::Result<()> {
    let attr = match fs::symlink_metadata(&entry.path) {
        Ok(attr) => attr,
        Err(e) => {
            warn!("{}", e);
            backup.report.increment("backup.error.stat", 1);
            return Ok(());
        }
    };
    if attr.is_file() {
        try!(backup_one_file(backup, &attr, entry));
    } else {
        // TODO: Backup directories, symlinks, etc.
        warn!("Skipping non-file {}", &entry.apath);
        backup.report.increment("backup.skipped.unsupported_file_kind", 1);
    }
    Ok(())
}


fn backup_one_file(backup: &mut Backup, attr: &fs::Metadata, entry: &sources::Entry) -> io::Result<()> {
    info!("backup {}", entry.path.display());

    let mut bw = BlockWriter::new();
    let mut f = try!(fs::File::open(&entry.path));
    try!(bw.copy_from_file(&mut f, attr.len(), &mut backup.report));
    let block_hash = try!(backup.block_dir.store(bw, &mut backup.report));
    backup.report.increment("backup.file.count", 1);

    assert!(apath::valid(&entry.apath), "invalid apath: {:?}", &entry.apath);

    // TODO: Get mtime.
    // TODO: Store list of blocks as well as whole-file hash?  Maybe not if it's not split?

    let index_entry = IndexEntry {
        apath: entry.apath.clone(),
        mtime: 0,
        kind: IndexKind::File,
        blake2b: block_hash,
    };
    backup.index_builder.push(index_entry);
    backup.index_builder.maybe_flush(&mut backup.report)
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::run_backup;
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

        // Directory is not stored yet, but should be.
        assert_eq!(1, report.get_count("backup.skipped.unsupported_file_kind"));

        let band_ids = af.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].as_string());

        let dur = report.get_duration("source.read");
        let read_us = (dur.subsec_nanos() as u64) / 1000u64 + dur.as_secs() * 1000000u64;
        assert!(read_us > 0);

        // TODO: Check band is closed.
        // TODO: List files in that band.
        // TODO: Check contents of that file.
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
        // Skipped both the directory and the symlink.
        assert_eq!(2, report.get_count("backup.skipped.unsupported_file_kind"));

        let band_ids = af.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].as_string());

        // TODO: Once implemented  check the symlink is included.
    }
}
