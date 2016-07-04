// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::fs;
use std::path::{Path};
use std::io;
use walkdir::WalkDir;

use super::archive::Archive;
use super::band::Band;
use super::block::{BlockDir, BlockWriter};
use super::report::Report;


pub fn run_backup(archive_path: &Path, sources: Vec<&Path>, mut report: &mut Report) -> io::Result<()> {
    // TODO: Sort the results: probably grouping together everything in a
    // directory, and then by file within that directory.
    let archive = Archive::open(archive_path).unwrap();
    let band = try!(archive.create_band());
    let mut block_dir = band.block_dir();
    for source_dir in sources {
        for entry in WalkDir::new(source_dir) {
            match entry {
                Ok(entry) => if entry.metadata().unwrap().is_file() {
                    backup_one(&mut block_dir, entry.path(), &mut report);
                },
                Err(e) => {
                    warn!("{}", e);
                }
            }
        }
    };
    Ok(())
}

fn backup_one(block_dir: &BlockDir, path: &Path, mut report: &mut Report) -> io::Result<()> {
    info!("backup {}", path.display());
    let mut bw = BlockWriter::new();
    let mut f = try!(fs::File::open(&path));
    try!(bw.copy_from_file(&mut f));
    let _hash = try!(block_dir.store(bw, &mut report));
    // TODO: Add to the index too.  Get the hash and length from the writer.
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate tempdir;

    use super::super::archive::scratch_archive;
    use super::super::report::Report;
    use super::run_backup;

    #[test]
    pub fn test_simple_backup() {
        let (_tempdir, archive) = scratch_archive();
        let srcdir = tempdir::TempDir::new("conserve-srcdir").unwrap();
        let mut report = Report::new();
        run_backup(archive.path(), vec![srcdir.path()], &mut report).unwrap();
        // TODO: list bands, should have one band.
        assert_eq!(0, report.get_count("block.write.count"));
    }
}
