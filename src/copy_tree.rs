// Conserve backup system.
// Copyright 2017, 2018, 2019 Martin Pool.

//! Copy tree contents.

use crate::*;

/// Copy files and other entries from one tree to another.
///
/// Progress and problems are reported to the source's report.
pub fn copy_tree<ST: ReadTree, DT: WriteTree>(source: &ST, dest: &mut DT) -> Result<()> {
    let report = source.report();
    // This causes us to walk the source tree twice, which is probably an acceptable option
    // since it's nice to see realistic overall progress. We could keep all the entries
    // in memory, and maybe we should, but it might get unreasonably big.
    report.set_phase("Measure source tree");
    report.set_total_work(source.size()?.file_bytes);
    report.set_phase("Copying");
    for entry in source.iter_entries(&report)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                // TODO: Show the filename that we failed to load: this requires changing the
                // `iter_entries` contract.
                report.problem(&format!("Error iterating source, continuing: {}", e));
                continue;
            }
        };
        report.start_entry(&entry);
        match entry.kind() {
            Kind::Dir => dest.write_dir(&entry),
            Kind::File => match source.file_contents(&entry) {
                Ok(mut content) => dest.write_file(&entry, &mut content),
                Err(e) => {
                    report.problem(&format!(
                        "Skipping unreadable source file {}: {}",
                        &entry.apath(),
                        e,
                    ));
                    // TODO: Count and accumulate problems.
                    continue;
                }
            },
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
