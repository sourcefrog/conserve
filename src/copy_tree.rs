// Conserve backup system.
// Copyright 2017, 2018, 2019 Martin Pool.

//! Copy tree contents.

#[allow(unused_imports)]
use snafu::ResultExt;

use crate::*;

const MEASURE_FIRST: bool = false;

/// Copy files and other entries from one tree to another.
///
/// Progress and problems are reported to the source's report.
pub fn copy_tree<ST: ReadTree, DT: WriteTree>(source: &ST, dest: &mut DT) -> Result<()> {
    let report = source.report();
    // This causes us to walk the source tree twice, which is probably an acceptable option
    // since it's nice to see realistic overall progress. We could keep all the entries
    // in memory, and maybe we should, but it might get unreasonably big.
    if MEASURE_FIRST {
        report.set_phase("Measure source tree");
        // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
        // again a second time? But, that'll potentially use memory proportional to tree size, which
        // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
        // deleted or changed while this is running.
        report.set_total_work(source.size()?.file_bytes);
    }
    report.set_phase("Copying");
    for entry in source.iter_entries(&report)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                report.show_error(&e);
                continue;
            }
        };
        report.start_entry(&entry);
        if let Err(e) = match entry.kind() {
            Kind::Dir => dest.write_dir(&entry),
            Kind::File => dest.copy_file(&entry, source),
            Kind::Symlink => dest.write_symlink(&entry),
            Kind::Unknown => {
                // TODO: Perhaps eventually we could backup and restore pipes, sockets, etc. Or at least
                // count them. For now, silently skip.
                continue;
            }
        } {
            report.show_error(&e);
            continue;
        }
        report.increment_work(entry.size().unwrap_or(0));
    }
    report.clear_phase();
    dest.finish()
}
