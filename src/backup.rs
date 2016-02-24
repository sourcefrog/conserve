// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

use std::path::{Path};
use std::io::{Result};
use walkdir::WalkDir;


pub fn run_backup(archive_path: &Path, sources: Vec<&Path>) -> Result<()> {
    // TODO: Sort the results.
    for source_dir in sources {
        for entry in WalkDir::new(source_dir) {
            // TODO: Just warn if not ok.
            info!("backup {}", entry.unwrap().path().display());
        }
    };
    Ok(())
}
