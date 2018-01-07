// Conserve backup system.
// Copyright 2017 Martin Pool.

//! Abstract Tree trait.

use super::*;

/// Abstract Tree that may be either on the real filesystem or stored in an archive.
pub trait ReadTree {
    type E: Entry;
    type I: Iterator<Item = Result<Self::E>>;
    type R: std::io::Read;

    fn iter_entries(&self, excludes: &GlobSet) -> Result<Self::I>;
    fn file_contents(&self, entry: &Self::E) -> Result<Self::R>;
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


pub fn copy_tree<ST: ReadTree, DT: WriteTree>(
    source: &ST, dest: &mut DT, excludes: &GlobSet) -> Result<()> {
    for entry in source.iter_entries(excludes)? {
        let entry = entry?;
        match entry.kind() {
            Kind::Dir => dest.write_dir(&entry),
            Kind::File => dest.write_file(&entry, &mut source.file_contents(&entry)?),
            Kind::Symlink => dest.write_symlink(&entry),
            Kind::Unknown => {
                warn!("Skipping unsupported file kind of {}", &entry.apath());
                // TODO: Count them - make the report visible somewhere? Or rather, make this the
                // job of the ST to skip them.
                continue;
            }
        }?;
    }
    dest.finish()
}
