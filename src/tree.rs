// Conserve backup system.
// Copyright 2017, 2018 Martin Pool.

//! Abstract Tree trait.

use super::*;

/// Abstract Tree that may be either on the real filesystem or stored in an archive.
pub trait ReadTree: HasReport {
    type E: Entry;
    type I: Iterator<Item = Result<Self::E>>;
    type R: std::io::Read;

    fn iter_entries(&self, report: &Report) -> Result<Self::I>;
    fn file_contents(&self, entry: &Self::E) -> Result<Self::R>;

    /// Estimate the number of entries in the tree.
    /// This might do somewhat expensive IO, so isn't the Iter's `size_hint`.
    fn estimate_count(&self) -> Result<u64>;

    /// Measure the tree size: typically requires walking all entries so
    /// takes a while.
    fn size(&self) -> Result<TreeSize> {
        let report = self.report();
        report.set_phase("Measuring");
        report.set_total_work(0);
        let mut tot = 0u64;
        for e in self.iter_entries(self.report())? {
            if let Some(s) = e?.size() {
                tot += s;
                report.increment_work(s);
            }
        }
        report.clear_phase();
        Ok(TreeSize {
            file_bytes: tot,
        })
    }
}

/// A tree open for writing, either local or an an archive.
///
/// This isn't a sub-trait of ReadTree since a backup band can't be read while writing is
/// still underway.
///
/// Entries must be written in Apath order, since that's a requirement of the index.
pub trait WriteTree {
    fn finish(&mut self) -> Result<()>;

    fn write_dir(&mut self, entry: &Entry) -> Result<()>;
    fn write_symlink(&mut self, entry: &Entry) -> Result<()>;
    fn write_file(&mut self, entry: &Entry, content: &mut std::io::Read) -> Result<()>;
}

/// The measured size of a tree.
pub struct TreeSize {
    pub file_bytes: u64,
}

/// Copy files and other entries from one tree to another.
///
/// Progress and problems are reported to the source's report.
pub fn copy_tree<ST: ReadTree, DT: WriteTree>(source: &ST, dest: &mut DT) -> Result<()> {
    let report = source.report();
    // This causes us to walk the source tree twice, which is probably an acceptable option
    // since it's nice to see realistic overall progress. We could keep all the entries
    // in memory, and maybe we should, but it might get unreasonably big.
    report.set_total_work(source.size()?.file_bytes);
    report.set_phase("Copying");
    for entry in source.iter_entries(&report)? {
        let entry = entry?;
        report.start_entry(&entry);
        match entry.kind() {
            Kind::Dir => dest.write_dir(&entry),
            Kind::File => dest.write_file(&entry, &mut source.file_contents(&entry)?),
            Kind::Symlink => dest.write_symlink(&entry),
            Kind::Unknown => {
                report.problem(&format!(
                    "Skipping unsupported file kind of {}",
                    &entry.apath()
                ));
                // TODO: Count them - make the report visible somewhere? Or rather, make this the
                // job of the ST to skip them.
                continue;
            }
        }?;
        report.increment_work(entry.size().unwrap_or(0));
    }
    report.clear_phase();
    dest.finish()
}
