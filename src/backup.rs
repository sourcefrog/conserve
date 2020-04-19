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
            basis_index,
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

    fn copy_dir<E: Entry>(&mut self, source_entry: &E) -> Result<()> {
        self.report.increment("dir", 1);
        self.push_entry(IndexEntry::metadata_from(source_entry))
    }

    /// Copy in the contents of a file from another tree.
    fn copy_file<R: ReadTree>(&mut self, source_entry: &R::Entry, from_tree: &R) -> Result<()> {
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
                // self.report.println(&format!("unchanged file {}", apath));

                // We can reasonably assume that the existing archive complies
                // with the archive invariants, which include that all the
                // blocks referenced by the index, are actually present.
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
                // self.report.println(&format!("changed file {}", apath));
            }
        }
        let addrs = if source_entry.size().map(|x| x > 0).unwrap_or(false) {
            let content = &mut from_tree.file_contents(&source_entry)?;
            self.store_files
                .store_file_content(&apath, content, &self.report)?
        } else {
            Vec::new()
        };
        let size = addrs.iter().map(|a| a.len).sum();
        self.report.increment_size(
            "file.bytes",
            Sizes {
                uncompressed: size,
                compressed: 0,
            },
        );
        self.push_entry(IndexEntry {
            addrs,
            ..IndexEntry::metadata_from(source_entry)
        })
    }

    fn copy_symlink<E: Entry>(&mut self, source_entry: &E) -> Result<()> {
        self.report.increment("symlink", 1);
        let target = source_entry.symlink_target().clone();
        assert!(target.is_some());
        self.push_entry(IndexEntry::metadata_from(source_entry))
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
        copy_tree(&lt, &mut bw, &COPY_DEFAULT).unwrap();
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
            .iter_entries(&report)
            .unwrap()
            .collect::<Vec<IndexEntry>>();
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
        copy_tree(&lt, &mut bw, &COPY_DEFAULT).unwrap();

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
        copy_tree(&srcdir.live_tree(), &mut bw, &COPY_DEFAULT).unwrap();

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
        copy_tree(&srcdir.live_tree(), &mut bw, &COPY_DEFAULT).unwrap();

        assert_eq!(report.get_count("file"), 2);
        assert_eq!(report.get_count("file.unchanged"), 0);

        // Make a second backup from the same tree, and we should see that
        // both files are unchanged.
        let mut bw = BackupWriter::begin(&af).unwrap();
        bw.report = Report::new();
        copy_tree(&srcdir.live_tree(), &mut bw, &COPY_DEFAULT).unwrap();

        assert_eq!(bw.report.get_count("file"), 2);
        assert_eq!(bw.report.get_count("file.unchanged"), 2);

        // Change one of the files, and in a new backup it should be recognized
        // as unchanged.
        srcdir.create_file_with_contents("bbb", b"longer content for bbb");

        let mut bw = BackupWriter::begin(&af).unwrap();
        bw.report = Report::new();
        copy_tree(&srcdir.live_tree(), &mut bw, &COPY_DEFAULT).unwrap();

        assert_eq!(bw.report.get_count("file"), 2);
        assert_eq!(bw.report.get_count("file.unchanged"), 1);
    }

    #[test]
    pub fn detect_minimal_mtime_change() {
        let af = ScratchArchive::new();
        let srcdir = TreeFixture::new();
        srcdir.create_file("aaa");
        srcdir.create_file_with_contents("bbb", b"longer content for bbb");

        let mut bw = BackupWriter::begin(&af).unwrap();
        let report = af.report();
        copy_tree(&srcdir.live_tree(), &mut bw, &COPY_DEFAULT).unwrap();

        assert_eq!(report.get_count("file"), 2);
        assert_eq!(report.get_count("file.unchanged"), 0);

        // Spin until the file's mtime is visibly different to what it was before.
        let bpath = srcdir.path().join("bbb");
        let orig_mtime = std::fs::metadata(&bpath).unwrap().modified().unwrap();
        loop {
            // Sleep a little while, so that even on systems with less than
            // nanosecond filesystem time resolution we can still see this is later.
            std::thread::sleep(std::time::Duration::from_millis(50));
            // Change one of the files, keeping the same length. If the mtime
            // changed, even fractionally, we should see the file was changed.
            srcdir.create_file_with_contents("bbb", b"woofer content for bbb");
            if std::fs::metadata(&bpath).unwrap().modified().unwrap() != orig_mtime {
                break;
            }
        }

        let mut bw = BackupWriter::begin(&af).unwrap();
        bw.report = Report::new();
        copy_tree(&srcdir.live_tree(), &mut bw, &COPY_DEFAULT).unwrap();

        assert_eq!(bw.report.get_count("file"), 2);
        assert_eq!(bw.report.get_count("file.unchanged"), 1);
    }
}
