use std::io;
use std::path::Path;

use super::archive::Archive;
use super::backup;
use super::report::Report;
use super::sources;

pub fn backup(archive: &str, source: &str, mut report: &mut Report) -> io::Result<()> {
    backup::run_backup(Path::new(archive), Path::new(source), &mut report)
}

pub fn init(archive: &str) -> io::Result<()> {
    Archive::init(Path::new(archive)).and(Ok(()))
}

pub fn list_source(source: &str, report: &mut Report) -> io::Result<()> {
    let _ = report;  // TODO: Pass into source iter.
    for entry in sources::iter(Path::new(source)) {
        println!("{}", entry.unwrap().apath);
    }
    Ok(())
}
