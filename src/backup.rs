// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::fs;
use std::path::{Path};
use std::io;
use walkdir::WalkDir;

use super::apath;
use super::archive::Archive;
use super::block::{BlockDir, BlockWriter};
use super::index::{IndexBuilder, IndexEntry, IndexKind};
use super::report::Report;


pub fn run_backup(archive_path: &Path, source: &Path, mut report: &mut Report)
    -> io::Result<()> {
    // TODO: Sort the results: probably grouping together everything in a
    // directory, and then by file within that directory.
    let archive = Archive::open(archive_path).unwrap();
    let band = try!(archive.create_band());
    let block_dir = band.block_dir();
    let mut index_builder = band.index_builder();
    // TODO: Clean error if source is a file not a directory.
    // TODO: Test the case where
    // TODO: Backup directories too.
    for entry in WalkDir::new(source) {
        match entry {
            Ok(entry) => if entry.metadata().unwrap().is_file() {
                try!(backup_one_file(&block_dir, &mut index_builder,
                        entry.path(), &mut report));
            },
            Err(e) => {
                warn!("{}", e);
            }
        }
    }
    try!(index_builder.finish_hunk(&mut report));
    // TODO: Mark band complete.
    Ok(())
}

fn backup_one_file(block_dir: &BlockDir, index_builder: &mut IndexBuilder,
    path: &Path, mut report: &mut Report) -> io::Result<()> {
    info!("backup {}", path.display());
    let mut bw = BlockWriter::new();
    let mut f = try!(fs::File::open(&path));
    try!(bw.copy_from_file(&mut f));
    let block_hash = try!(block_dir.store(bw, &mut report));
    report.increment("backup.file.count", 1);

    // TODO: Get the whole path relative to the top level source directory.
    let mut apath = String::from("/");
    apath.push_str(path.file_name().unwrap().to_str().unwrap());
    assert!(apath::valid(&apath));

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
        // TODO: list bands, should have one band.
        // TODO: List files in that band.
        // TODO: Check contents of that file.
        assert_eq!(1, report.get_count("block.write.count"));
        assert_eq!(1, report.get_count("backup.file.count"));
    }
}
