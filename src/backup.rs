// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

use super::*;

/// Accepts files to write in the archive (in apath order.)
#[derive(Debug)]
pub struct BackupWriter {
    band: Band,
    block_dir: BlockDir,
    index_builder: IndexBuilder,
    report: Report,
}

impl BackupWriter {
    /// Create a new BackupWriter.
    ///
    /// This currently makes a new top-level band.
    pub fn begin(archive: &Archive) -> Result<BackupWriter> {
        let band = Band::create(archive)?;
        let block_dir = archive.block_dir().clone();
        let index_builder = band.index_builder();
        Ok(BackupWriter {
            band,
            block_dir,
            index_builder,
            report: archive.report().clone(),
        })
    }

    fn push_entry(&mut self, index_entry: IndexEntry) -> Result<()> {
        self.index_builder.push(index_entry);
        self.index_builder.maybe_flush(&self.report)?;
        Ok(())
    }
}

impl tree::WriteTree for BackupWriter {
    fn finish(&mut self) -> Result<()> {
        self.index_builder.finish_hunk(&self.report)?;
        self.band.close(&self.report)?;
        Ok(())
    }

    fn write_dir(&mut self, source_entry: &dyn Entry) -> Result<()> {
        self.report.increment("dir", 1);
        self.push_entry(IndexEntry {
            apath: String::from(source_entry.apath()),
            mtime: source_entry.unix_mtime(),
            kind: Kind::Dir,
            addrs: vec![],
            target: None,
        })
    }

    fn write_file(
        &mut self,
        source_entry: &dyn Entry,
        content: &mut dyn std::io::Read,
    ) -> Result<()> {
        self.report.increment("file", 1);
        // TODO: Cope graciously if the file disappeared after readdir.
        let addrs = self.block_dir.store(content, &self.report)?;
        let bytes = addrs.iter().map(|a| a.len).sum();
        self.report.increment_size(
            "file.bytes",
            Sizes {
                uncompressed: bytes,
                compressed: 0,
            },
        );
        // TODO: Perhaps return a future for an index, so that storage of the files can overlap.
        self.push_entry(IndexEntry {
            apath: String::from(source_entry.apath()),
            mtime: source_entry.unix_mtime(),
            kind: Kind::File,
            addrs,
            target: None,
        })
    }

    fn write_symlink(&mut self, source_entry: &dyn Entry) -> Result<()> {
        self.report.increment("symlink", 1);
        let target = source_entry.symlink_target().clone();
        assert!(target.is_some());
        self.push_entry(IndexEntry {
            apath: String::from(source_entry.apath()),
            mtime: source_entry.unix_mtime(),
            kind: Kind::Symlink,
            addrs: vec![],
            target,
        })
    }
}

impl HasReport for BackupWriter {
    fn report(&self) -> &Report {
        &self.report
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::test_fixtures::{ScratchArchive, TreeFixture};

    #[cfg(unix)]
    #[test]
    pub fn symlink() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_symlink("symlink", "/a/broken/destination");
        let lt = LiveTree::open(srcdir.path(), &Report::new()).unwrap();
        let mut bw = BackupWriter::begin(&af).unwrap();
        let report = af.report();
        copy_tree(&lt, &mut bw).unwrap();
        assert_eq!(0, report.get_count("block.write"));
        assert_eq!(0, report.get_count("file"));
        assert_eq!(1, report.get_count("symlink"));
        assert_eq!(0, report.get_count("skipped.unsupported_file_kind"));

        let band_ids = af.list_bands().unwrap();
        assert_eq!(1, band_ids.len());
        assert_eq!("b0000", band_ids[0].to_string());

        let band = Band::open(&af, &band_ids[0]).unwrap();
        assert!(band.is_closed().unwrap());

        let index_entries = band
            .index()
            .iter(&excludes::excludes_nothing(), &report)
            .unwrap()
            .filter_map(|i| i.ok())
            .collect::<Vec<IndexEntry>>();
        assert_eq!(2, index_entries.len());

        let e2 = &index_entries[1];
        assert_eq!(e2.kind(), Kind::Symlink);
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

        let report = af.report();
        let excludes = excludes::from_strings(&["/**/foo*", "/**/baz"]).unwrap();
        let lt = LiveTree::open(srcdir.path(), &report)
            .unwrap()
            .with_excludes(excludes);
        let mut bw = BackupWriter::begin(&af).unwrap();
        copy_tree(&lt, &mut bw).unwrap();

        assert_eq!(1, report.get_count("block.write"));
        assert_eq!(1, report.get_count("file"));
        assert_eq!(2, report.get_count("dir"));
        assert_eq!(0, report.get_count("symlink"));
        assert_eq!(0, report.get_count("skipped.unsupported_file_kind"));

        // These are turned off because current copy_tree walks the tree twice.
        // assert_eq!(4, report.get_count("skipped.excluded.files"));
        // assert_eq!(1, report.get_count("skipped.excluded.directories"));
    }

    #[test]
    pub fn empty_file_uses_zero_blocks() {
        use std::io::Read;

        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_file_with_contents("empty", &[]);
        let mut bw = BackupWriter::begin(&af).unwrap();
        let report = af.report();
        copy_tree(&srcdir.live_tree(), &mut bw).unwrap();

        assert_eq!(0, report.get_count("block.write"));
        assert_eq!(1, report.get_count("file"), "file count");

        // Read back the empty file
        let st = StoredTree::open_last(&af).unwrap();
        let empty_entry = st
            .iter_entries(&af.report())
            .unwrap()
            .map(|i| i.unwrap())
            .find(|ref i| i.apath == "/empty")
            .expect("found one entry");
        let mut sf = st.file_contents(&empty_entry).unwrap();
        let mut s = String::new();
        assert_eq!(sf.read_to_string(&mut s).unwrap(), 0);
        assert_eq!(s.len(), 0);
    }
}
