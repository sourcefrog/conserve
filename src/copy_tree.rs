// Conserve backup system.
// Copyright 2017, 2018, 2019, 2020 Martin Pool.

//! Copy tree contents.

#[allow(unused_imports)]
use snafu::ResultExt;

use crate::*;

#[derive(Default, Clone, Debug)]
pub struct CopyOptions {
    pub print_filenames: bool,
    pub measure_first: bool,
}

pub const COPY_DEFAULT: CopyOptions = CopyOptions {
    print_filenames: false,
    measure_first: false,
};

/// Statistics about a tree copy operation.
#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub struct CopyStats {
    /// Number of entries skipped because they're an
    pub unknown_kind_count: u64,

    pub dir_count: u64,
    pub file_count: u64,
    pub symlink_count: u64,

    pub copies_failed: u64,

    // TODO: Be clearer what this is measuring.
    pub file_totals: Sizes,
}

// TODO: Summarize to an io::Write the contents of the CopyStats,
// maybe differently for backup vs restore.

/// Copy files and other entries from one tree to another.
///
/// Progress and problems are reported to the source's report.
pub fn copy_tree<ST: ReadTree, DT: WriteTree>(
    source: &ST,
    dest: &mut DT,
    options: &CopyOptions,
) -> Result<CopyStats> {
    let report = source.report();
    let mut stats = CopyStats::default();
    // This causes us to walk the source tree twice, which is probably an acceptable option
    // since it's nice to see realistic overall progress. We could keep all the entries
    // in memory, and maybe we should, but it might get unreasonably big.
    if options.measure_first {
        report.set_phase("Measure source tree");
        // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
        // again a second time? But, that'll potentially use memory proportional to tree size, which
        // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
        // deleted or changed while this is running.
        report.set_total_work(source.size()?.file_bytes);
    }
    report.set_phase("Copying");
    for entry in source.iter_entries(&report)? {
        let apath = entry.apath();
        if options.print_filenames {
            report.println(apath);
        }
        report.start_entry(apath);
        if let Err(e) = match entry.kind() {
            Kind::Dir => {
                stats.dir_count += 1;
                dest.copy_dir(&entry)
            }
            Kind::File => {
                stats.file_count += 1;
                dest.copy_file(&entry, source)
                    .map(|sizes| stats.file_totals += sizes)
            }
            Kind::Symlink => {
                stats.symlink_count += 1;
                dest.copy_symlink(&entry)
            }
            Kind::Unknown => {
                stats.unknown_kind_count += 1;
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                continue;
            }
        } {
            stats.copies_failed += 1;
            report.show_error(&e);
            continue;
        }
        report.increment_work(entry.size().unwrap_or(0));
    }
    report.clear_phase();
    dest.finish()?;
    Ok(stats)
}
