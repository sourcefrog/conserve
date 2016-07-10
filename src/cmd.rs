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
    let mut source_iter = sources::iter(Path::new(source));
    for entry in &mut source_iter {
        println!("{}", try!(entry).apath);
    }
    report.merge_from(&source_iter.get_report());
    Ok(())
}
