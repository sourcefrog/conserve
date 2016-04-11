// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::path::{Path};
use std::io::{Result};
use walkdir::WalkDir;

use super::Archive;


pub fn run_backup(archive_path: &Path, sources: Vec<&Path>) -> Result<()> {
    // TODO: Sort the results: probably grouping together everything in a
    // directory, and then by file within that directory.
    Archive::open(archive_path).unwrap();
    for source_dir in sources {
        for entry in WalkDir::new(source_dir) {
            match entry {
                Ok(entry) => if entry.metadata().unwrap().is_file() {
                    backup_one(entry.path());
                },
                Err(e) => {
                    warn!("{}", e);
                }
            }
        }
    };
    Ok(())
}

fn backup_one(path: &Path) {
    info!("backup {}", path.display());
}

#[cfg(test)]
mod tests {
    extern crate tempdir;
    
    use super::super::archive::scratch_archive;
    use super::run_backup;
    
    #[test]
    pub fn test_simple_backup() {
        let (_tempdir, archive) = scratch_archive();
        let srcdir = tempdir::TempDir::new("conserve-srcdir").unwrap();
        run_backup(archive.path(), vec![srcdir.path()]).unwrap();
    }
}
