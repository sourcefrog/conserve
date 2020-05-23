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

/// Copy files and other entries from one tree to another.
///
/// Progress and problems are reported to the source's report.
pub fn copy_tree<ST: ReadTree, DT: WriteTree>(
    source: &ST,
    dest: &mut DT,
    options: &CopyOptions,
) -> Result<()> {
    let report = source.report();
    // This causes us to walk the source tree twice, which is probably an acceptable option
    // since it's nice to see realistic overall progress. We could keep all the entries
    // in memory, and maybe we should, but it might get unreasonably big.
    if options.measure_first {
        ui::set_progress_phase("Measure source tree");
        // TODO: Maybe read all entries for the source tree in to memory now, rather than walking it
        // again a second time? But, that'll potentially use memory proportional to tree size, which
        // I'd like to avoid, and also perhaps make it more likely we grumble about files that were
        // deleted or changed while this is running.
        ui::set_bytes_total(source.size()?.file_bytes);
    }
    ui::set_progress_phase("Copying");
    for entry in source.iter_entries(&report)? {
        if options.print_filenames {
            crate::ui::println(entry.apath());
        }
        ui::set_progress_file(entry.apath());
        if let Err(e) = match entry.kind() {
            Kind::Dir => dest.copy_dir(&entry),
            Kind::File => dest.copy_file(&entry, source),
            Kind::Symlink => dest.copy_symlink(&entry),
            Kind::Unknown => {
                // TODO: Perhaps eventually we could backup and restore pipes,
                // sockets, etc. Or at least count them. For now, silently skip.
                // https://github.com/sourcefrog/conserve/issues/82
                continue;
            }
        } {
            ui::show_error(&e);
            continue;
        }
        ui::increment_bytes_done(entry.size().unwrap_or(0));
    }
    ui::clear_progress();
    dest.finish()
}
