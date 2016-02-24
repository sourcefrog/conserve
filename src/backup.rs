// Conserve backup system.
// Copyright 2015 Martin Pool.

use std;
use std::fs::{File};
use std::io::{Error, Result, Write};
use std::path::{Path, PathBuf} ;

use rustc_serialize::json;

pub fn run_backup(archive_path: &Path, sources: Vec<&Path>) -> Result<()> {
    Ok(())
}
