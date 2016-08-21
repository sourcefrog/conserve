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


pub fn run_backup(archive_path: &Path, source: &Path, mut report: &mut Report)
    -> io::Result<()> {
    // TODO: More tests.
    // TODO: Backup directories and symlinks too.

    let archive = Archive::open(archive_path).unwrap();
    let band = try!(archive.create_band());
    let block_dir = band.block_dir();
    let mut index_builder = band.index_builder();

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
        let attr = match fs::symlink_metadata(&entry.path) {
            Ok(attr) => attr,
            Err(e) => {
                warn!("{}", e);
                return Err(e);
            }
        };
        if attr.is_file() {
            try!(backup_one_file(&block_dir, &mut index_builder,
                &entry.path, entry.apath, &mut report));
        } else {
            // TODO: Backup directories, symlinks, etc.
            warn!("Skipping non-file {}", &entry.apath);
            report.increment("backup.skipped.unsupported_file_kind", 1);
        }
    }
    try!(index_builder.finish_hunk(&mut report));
    try!(band.close());
    Ok(())
}

fn backup_one_file(block_dir: &BlockDir, index_builder: &mut IndexBuilder,
    path: &Path, apath: String, mut report: &mut Report) -> io::Result<()> {
    info!("backup {}", path.display());

    let mut bw = BlockWriter::new();
    let mut f = try!(fs::File::open(&path));
    try!(bw.copy_from_file(&mut f));
    let block_hash = try!(block_dir.store(bw, &mut report));
    report.increment("backup.file.count", 1);

    assert!(apath::valid(&apath), "invalid apath: {:?}", &apath);

    // TODO: Get mtime.
    // TODO: Store list of blocks as well as whole-file hash?  Maybe not if it's not split?

    let index_entry = IndexEntry {
        apath: apath,
        mtime: 0,
        kind: IndexKind::File,
        blake2b: block_hash,
    };
    index_builder.push(index_entry);

    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::run_backup;
    use super::super::archive::scratch_archive;
    use super::super::report::Report;
    use super::super::testfixtures::TreeFixture;

    #[test]
    pub fn simple_backup() {
        let (_tempdir, archive) = scratch_archive();
        let srcdir = TreeFixture::new();
        srcdir.create_file("hello");
        let mut report = Report::new();
        run_backup(archive.path(), srcdir.path(), &mut report).unwrap();
        assert_eq!(1, report.get_count("block.write.count"));
        assert_eq!(1, report.get_count("backup.file.count"));

        // Directory is not stored yet, but should be.
        assert_eq!(1, report.get_count("backup.skipped.unsupported_file_kind"));

        let band_ids = archive.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].as_string());

        // TODO: Check band is closed.
        // TODO: List files in that band.
        // TODO: Check contents of that file.
    }

    #[cfg(unix)]
    #[test]
    pub fn symlink() {
        let (_tempdir, archive) = scratch_archive();
        let srcdir = TreeFixture::new();
        srcdir.create_symlink("symlink", "/a/broken/destination");
        let mut report = Report::new();
        run_backup(archive.path(), srcdir.path(), &mut report).unwrap();
        assert_eq!(0, report.get_count("block.write.count"));
        assert_eq!(0, report.get_count("backup.file.count"));
        // Skipped both the directory and the symlink.
        assert_eq!(2, report.get_count("backup.skipped.unsupported_file_kind"));

        let band_ids = archive.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].as_string());

        // TODO: Once implemented  check the symlink is included.
    }
}
