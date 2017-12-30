// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use std::fs;
use std::path::Path;

use super::*;
use index;
use sources;

use globset::GlobSet;

#[derive(Debug)]
pub struct BackupOptions {
    excludes: GlobSet,
}


impl BackupOptions {
    pub fn default() -> Self {
        BackupOptions { excludes: excludes::excludes_nothing() }
    }

    pub fn with_excludes(self, exclude: Vec<&str>) -> Result<Self> {
        Ok(BackupOptions {
            excludes: excludes::from_strings(exclude)?,
            ..self
        })
    }
}


/// Accepts files to write in the archive (in apath order.)
#[derive(Debug)]
struct BackupWriter {
    band: Band,
    block_dir: BlockDir,
    index_builder: IndexBuilder,
    report: Report,
}


/// Make a new backup from a source tree into a band in this archive.
pub fn make_backup(source: &Path, archive: &Archive, backup_options: &BackupOptions) -> Result<()> {
    let mut backup_writer = BackupWriter::begin_band(archive)?;
    for entry in sources::iter(source, &backup_writer.report, &backup_options.excludes)? {
        backup_writer.store(&entry?)?;
    }
    backup_writer.finish()
}


impl BackupWriter {
    fn begin_band(archive: &Archive) -> Result<BackupWriter> {
        let band = archive.create_band()?;
        let block_dir = band.block_dir();
        let index_builder = band.index_builder();
        Ok(BackupWriter {
            band: band,
            block_dir: block_dir,
            index_builder: index_builder,
            report: archive.report().clone(),
        })
    }

    fn finish(mut self) -> Result<()> {
        self.index_builder.finish_hunk(&self.report)?;
        self.band.close(&self.report)?;
        Ok(())
    }

    fn store(&mut self, source_entry: &sources::Entry) -> Result<()> {
        info!("Backup {}", source_entry.path.display());
        let store_fn = if source_entry.metadata.is_file() {
            BackupWriter::store_file
        } else if source_entry.metadata.is_dir() {
            BackupWriter::store_dir
        } else if source_entry.metadata.file_type().is_symlink() {
            BackupWriter::store_symlink
        } else {
            warn!("Skipping unsupported file kind {}", &source_entry.apath);
            self.report.increment("skipped.unsupported_file_kind", 1);
            return Ok(());
        };
        let new_index_entry = store_fn(self, source_entry)?;
        self.index_builder.push(new_index_entry);
        self.index_builder.maybe_flush(&self.report)?;
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
        let mut f = fs::File::open(&source_entry.path)?;
        let (addrs, body_hash) = self.block_dir.store(&mut f, &self.report)?;
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
        // TODO: Record a problem and log a message if the target is not decodable, rather than
        //  silently losing.
        let target = fs::read_link(&source_entry.path)?
            .to_string_lossy()
            .to_string();
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
    use super::super::*;
    use test_fixtures::{ScratchArchive, TreeFixture};

    #[cfg(unix)]
    #[test]
    pub fn symlink() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_symlink("symlink", "/a/broken/destination");
        make_backup(srcdir.path(), &af, &BackupOptions::default())
            .unwrap();
        let report = af.report();
        assert_eq!(0, report.get_count("block.write"));
        assert_eq!(0, report.get_count("file"));
        assert_eq!(1, report.get_count("symlink"));
        assert_eq!(0, report.get_count("skipped.unsupported_file_kind"));

        let band_ids = af.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].as_string());

        let band = af.open_band(&Some(band_ids[0].clone())).unwrap();
        assert!(band.is_closed().unwrap());

        let index_entries = band.index_iter(&excludes::excludes_nothing(), &report)
            .unwrap()
            .filter_map(|i| i.ok())
            .collect::<Vec<index::Entry>>();
        assert_eq!(2, index_entries.len());

        let e2 = &index_entries[1];
        assert_eq!(e2.kind, index::IndexKind::Symlink);
        assert_eq!(e2.apath, "/symlink");
        assert_eq!(e2.target.as_ref().unwrap(), "/a/broken/destination");
    }

    #[test]
    pub fn excludes() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();

        srcdir.create_dir("test");
        srcdir.create_dir("foooooo");
        srcdir.create_file("foo");
        srcdir.create_file("fooBar");
        srcdir.create_file("foooooo/test");
        srcdir.create_file("test/baz");
        srcdir.create_file("baz");
        srcdir.create_file("bar");

        let backup_options = BackupOptions::default()
            .with_excludes(vec!["/**/foo*", "/**/baz"]).unwrap();
        make_backup(srcdir.path(), &af, &backup_options).unwrap();
        let report = af.report();

        assert_eq!(1, report.get_count("block.write"));
        assert_eq!(1, report.get_count("file"));
        assert_eq!(2, report.get_count("dir"));
        assert_eq!(0, report.get_count("symlink"));
        assert_eq!(0, report.get_count("skipped.unsupported_file_kind"));
        assert_eq!(4, report.get_count("skipped.excluded.files"));
        assert_eq!(1, report.get_count("skipped.excluded.directories"));
    }

    #[test]
    pub fn empty_file_uses_zero_blocks() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_file_with_contents("empty", &[]);
        make_backup(srcdir.path(), &af, &BackupOptions::default()).unwrap();
        let report = af.report();

        assert_eq!(0, report.get_count("block.write"));
        assert_eq!(1, report.get_count("file"), "file count");

        // Read back the empty file
        let st = StoredTree::open(&af, &None).unwrap();
        let empty_entry = st.index_iter(&excludes::excludes_nothing())
            .unwrap()
            .map(|i| i.unwrap())
            .find(|ref i| i.apath == "/empty")
            .expect("found one entry");
        let sf = st.file_contents(&empty_entry).unwrap();
        assert_eq!(0, sf.count(), "reading empty file has zero chunks");
    }
}
