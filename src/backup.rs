// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018, 2019, 2020 Martin Pool.

//! Make a backup by walking a source directory and copying the contents
//! into an archive.

#[allow(unused_imports)]
use snafu::ResultExt;

use super::blockdir::StoreFiles;
use super::*;
use crate::index::IndexEntryIter;

/// Accepts files to write in the archive (in apath order.)
pub struct BackupWriter {
    band: Band,
    index_builder: IndexBuilder,
    report: Report,
    store_files: StoreFiles,
    block_dir: BlockDir,

    /// The index for the last stored band, used as hints for whether newly
    /// stored files have changed.
    basis_index: Option<IndexEntryIter>,
}

impl BackupWriter {
    /// Create a new BackupWriter.
    ///
    /// This currently makes a new top-level band.
    pub fn begin(archive: &Archive) -> Result<BackupWriter> {
        let basis_index = archive
            .last_complete_band()?
            .map(|b| b.iter_entries(&archive.report()))
            .transpose()?;
        // Create the new band only after finding the basis band!
        let band = Band::create(archive)?;
        let index_builder = band.index_builder();
        Ok(BackupWriter {
            band,
            index_builder,
            report: archive.report().clone(),
            store_files: StoreFiles::new(archive.block_dir().clone()),
            block_dir: archive.block_dir().clone(),
            basis_index,
        })
    }

    fn push_entry(&mut self, index_entry: Entry) -> Result<()> {
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

    fn write_dir(&mut self, source_entry: &Entry) -> Result<()> {
        self.report.increment("dir", 1);
        self.push_entry(Entry {
            apath: source_entry.apath().clone(),
            mtime: source_entry.unix_mtime(),
            kind: Kind::Dir,
            addrs: vec![],
            target: None,
            size: None,
        })
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file<R: ReadTree>(&mut self, source_entry: &Entry, from_tree: &R) -> Result<()> {
        self.report.increment("file", 1);
        let apath = source_entry.apath();
        if let Some(basis_entry) = self
            .basis_index
            .as_mut()
            .map(|bi| bi.advance_to(&apath))
            .flatten()
        {
            if source_entry.is_unchanged_from(&basis_entry) {
                // TODO: In verbose mode, say if the file is changed, unchanged,
                // etc, but without duplicating the filenames.
                //
                // self.report.print(&format!("unchanged file {}", apath));
                if self.block_dir.contains_all_blocks(&basis_entry.addrs) {
                    self.report.increment("file.unchanged", 1);
                    self.report.increment_size(
                        "file.bytes",
                        Sizes {
                            uncompressed: source_entry.size().unwrap_or_default(),
                            compressed: 0,
                        },
                    );
                    return self.push_entry(basis_entry);
                } else {
                    self.report.problem(&format!(
                        "Some blocks of basis file {} are missing from the blockdir; writing them again", apath));
                }
            } else {
                // self.report.print(&format!("changed file {}", apath));
            }
        }
        let content = &mut from_tree.file_contents(&source_entry)?;
        let addrs = self
            .store_files
            .store_file_content(&apath, content, &self.report)?;
        let size = addrs.iter().map(|a| a.len).sum();
        self.report.increment_size(
            "file.bytes",
            Sizes {
                uncompressed: size,
                compressed: 0,
            },
        );
        self.push_entry(Entry {
            apath: apath.clone(),
            mtime: source_entry.unix_mtime(),
            kind: Kind::File,
            addrs,
            target: None,
            size: Some(size),
        })
    }

    fn write_symlink(&mut self, source_entry: &Entry) -> Result<()> {
        self.report.increment("symlink", 1);
        let target = source_entry.symlink_target().clone();
        assert!(target.is_some());
        self.push_entry(Entry {
            apath: source_entry.apath().clone(),
            mtime: source_entry.unix_mtime(),
            kind: Kind::Symlink,
            addrs: vec![],
            target,
            size: None,
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

        let index_entries = band.iter_entries(&report).unwrap().collect::<Vec<Entry>>();
        assert_eq!(2, index_entries.len());

        let e2 = &index_entries[1];
        assert_eq!(e2.kind(), Kind::Symlink);
        assert_eq!(&e2.apath, "/symlink");
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
            .find(|ref i| &i.apath == "/empty")
            .expect("found one entry");
        let mut sf = st.file_contents(&empty_entry).unwrap();
        let mut s = String::new();
        assert_eq!(sf.read_to_string(&mut s).unwrap(), 0);
        assert_eq!(s.len(), 0);
    }

    #[test]
    pub fn detect_unchanged() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_file("aaa");
        srcdir.create_file("bbb");

        let mut bw = BackupWriter::begin(&af).unwrap();
        let report = af.report();
        copy_tree(&srcdir.live_tree(), &mut bw).unwrap();

        assert_eq!(report.get_count("file"), 2);
        assert_eq!(report.get_count("file.unchanged"), 0);

        // Make a second backup from the same tree, and we should see that
        // both files are unchanged.
        let mut bw = BackupWriter::begin(&af).unwrap();
        bw.report = Report::new();
        copy_tree(&srcdir.live_tree(), &mut bw).unwrap();

        assert_eq!(bw.report.get_count("file"), 2);
        assert_eq!(bw.report.get_count("file.unchanged"), 2);

        // Change one of the files; we should now see it has changed.
        //
        // There is a possibility of a race if the file is changed within the granularity of the mtime, without the size changing.
        // The proper fix for that is to store a more precise mtime
        // <https://github.com/sourcefrog/conserve/issues/81>. To avoid
        // it for now, we'll make sure the length changes.
        srcdir.create_file_with_contents("bbb", b"longer content for bbb");

        let mut bw = BackupWriter::begin(&af).unwrap();
        bw.report = Report::new();
        copy_tree(&srcdir.live_tree(), &mut bw).unwrap();

        assert_eq!(bw.report.get_count("file"), 2);
        assert_eq!(bw.report.get_count("file.unchanged"), 1);
    }
}
