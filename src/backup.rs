// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::fs;
use std::path::Path;

use super::archive::Archive;
use super::block::BlockDir;
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


pub fn backup(archive_path: &Path, source: &Path, report: &Report) -> Result<()> {
    let archive = try!(Archive::open(archive_path, &report));
    let band = try!(archive.create_band(&report));
    let mut backup = Backup {
        block_dir: band.block_dir(),
        index_builder: band.index_builder(),
        report: report.clone(),
    };
    let source_iter = try!(sources::iter(source, report));
    for entry in source_iter {
        try!(backup.store_one_source_entry(&try!(entry)));
    }
    try!(backup.index_builder.finish_hunk(report));
    try!(band.close(&backup.report));
    Ok(())
}


impl Backup {
    fn store_one_source_entry(&mut self, source_entry: &sources::Entry) -> Result<()> {
        info!("Backup {}", source_entry.path.display());
        let store_fn = if source_entry.metadata.is_file() {
            Backup::store_file
        } else if source_entry.metadata.is_dir() {
            Backup::store_dir
        } else if source_entry.metadata.file_type().is_symlink() {
            Backup::store_symlink
        } else {
            warn!("Skipping unsupported file kind {}", &source_entry.apath);
            self.report.increment("skipped.unsupported_file_kind", 1);
            return Ok(());
        };
        let new_index_entry = try!(store_fn(self, source_entry));
        self.index_builder.push(new_index_entry);
        try!(self.index_builder.maybe_flush(&self.report));
        Ok(())
    }


    fn store_dir(&mut self, source_entry: &sources::Entry) -> Result<index::Entry> {
        self.report.increment("dir", 1);
        Ok(index::Entry {
            apath: source_entry.apath.to_string().clone(),
            mtime: source_entry.unix_mtime(),
            kind: IndexKind::Dir,
            addrs: vec![],
            blake2b: None,
            target: None,
        })
    }


    fn store_file(&mut self, source_entry: &sources::Entry) -> Result<index::Entry> {
        self.report.increment("file", 1);
        // TODO: Cope graciously if the file disappeared after readdir.
        let mut f = try!(fs::File::open(&source_entry.path));
        let (addrs, body_hash) = try!(self.block_dir.store(&mut f, &self.report));
        Ok(index::Entry {
            apath: source_entry.apath.to_string().clone(),
            mtime: source_entry.unix_mtime(),
            kind: IndexKind::File,
            addrs: addrs,
            blake2b: Some(body_hash),
            target: None,
        })
    }


    fn store_symlink(&mut self, source_entry: &sources::Entry) -> Result<index::Entry> {
        self.report.increment("symlink", 1);
        // TODO: Maybe log a warning if the target is not decodable rather than silently
        // losing.
        let target = try!(fs::read_link(&source_entry.path)).to_string_lossy().to_string();
        Ok(index::Entry {
            apath: source_entry.apath.to_string().clone(),
            mtime: source_entry.unix_mtime(),
            kind: IndexKind::Symlink,
            addrs: vec![],
            blake2b: None,
            target: Some(target),
        })
    }
}


#[cfg(test)]
mod tests {
    extern crate tempdir;

    #[cfg(unix)]
    #[test]
    pub fn symlink() {
        use super::backup;
        use index;
        use report::Report;
        use testfixtures::{ScratchArchive, TreeFixture};

        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_symlink("symlink", "/a/broken/destination");
        let report = Report::new();
        backup(af.path(), srcdir.path(), &report).unwrap();
        assert_eq!(0, report.borrow_counts().get_count("block"));
        assert_eq!(0, report.borrow_counts().get_count("file"));
        assert_eq!(1, report.borrow_counts().get_count("symlink"));
        assert_eq!(0,
                   report.borrow_counts().get_count("skipped.unsupported_file_kind"));

        let band_ids = af.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].as_string());

        let band = af.open_band(&band_ids[0], &report).unwrap();
        assert!(band.is_closed().unwrap());

        let index_entries = band.index_iter(&report)
            .unwrap()
            .filter_map(|i| i.ok())
            .collect::<Vec<index::Entry>>();
        assert_eq!(2, index_entries.len());

        let e2 = &index_entries[1];
        assert_eq!(e2.kind, index::IndexKind::Symlink);
        assert_eq!(e2.apath, "/symlink");
        assert_eq!(e2.target.as_ref().unwrap(), "/a/broken/destination");
    }
}
