// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

//! Terminal/text UI.

use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use lazy_static::lazy_static;
#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn, Level};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;

use crate::progress::Progress;

lazy_static! {
    /// A global Nutmeg view.
    ///
    /// This is global to reflect that there is globally one stdout/stderr:
    /// this object manages it.
    static ref NUTMEG_VIEW: nutmeg::View<Progress> =
        nutmeg::View::new(Progress::None, nutmeg::Options::default()
            .destination(nutmeg::Destination::Stderr));
}
/// Chosen style of timestamp prefix on trace lines.
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum TraceTimeStyle {
    /// No timestamp on trace lines.
    None,
    /// Universal time, in RFC 3339 style.
    Utc,
    /// Local time, in RFC 3339, using the offset when the program starts.
    Local,
    /// Time since the start of the process, in seconds.
    Relative,
}

/// Should progress be enabled for ad-hoc created Nutmeg views.
// (These should migrate to NUTMEG_VIEW.)
static PROGRESS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable_tracing(
    time_style: &TraceTimeStyle,
    console_level: Level,
    json_path: &Option<PathBuf>,
) {
    use tracing_subscriber::fmt::time;
    fn hookup<FT>(timer: FT, console_level: Level, json_path: &Option<PathBuf>)
    where
        FT: FormatTime + Send + Sync + 'static,
    {
        let console_layer = tracing_subscriber::fmt::Layer::default()
            .with_ansi(clicolors_control::colors_enabled())
            .with_writer(WriteToNutmeg)
            .with_timer(timer)
            .with_filter(LevelFilter::from_level(console_level));
        // TODO: Maybe tracing_appender instead?
        let json_layer = json_path
            .as_ref()
            .map(|path| {
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .write(true)
                    .read(false)
                    .open(path)
                    .expect("open json log file")
            })
            .map(|w| {
                tracing_subscriber::fmt::Layer::default()
                    .json()
                    .with_writer(w)
            });
        Registry::default()
            .with(console_layer)
            .with(crate::trace_counter::CounterLayer())
            .with(json_layer)
            .init();
    }

    match time_style {
        TraceTimeStyle::None => hookup((), console_level, json_path),
        TraceTimeStyle::Utc => hookup(time::UtcTime::rfc_3339(), console_level, json_path),
        TraceTimeStyle::Relative => hookup(time::uptime(), console_level, json_path),
        TraceTimeStyle::Local => hookup(
            time::OffsetTime::local_rfc_3339().unwrap(),
            console_level,
            json_path,
        ),
    }
    trace!("Tracing enabled");
}

struct WriteToNutmeg();

impl io::Write for WriteToNutmeg {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        NUTMEG_VIEW.message_bytes(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Enable drawing progress bars, only if stdout is a tty.
///
/// Progress bars are off by default.
pub fn enable_progress(enabled: bool) {
    PROGRESS_ENABLED.store(enabled, Ordering::Relaxed);
    if enabled {
        NUTMEG_VIEW.resume();
    } else {
        NUTMEG_VIEW.suspend();
    }
}

// #[deprecated]: Use the global view instead.
pub(crate) fn nutmeg_options() -> nutmeg::Options {
    nutmeg::Options::default().progress_enabled(PROGRESS_ENABLED.load(Ordering::Relaxed))
}

/// Show progress on the global terminal progress bar,
/// or clear the bar if it's [Progress::None].
pub(crate) fn post_progress(progress: Progress) {
    if matches!(progress, Progress::None) {
        // Hide the progress bar.
        // TODO: suspend and update may not be needed if it renders to nothing?
        NUTMEG_VIEW.suspend();
        NUTMEG_VIEW.update(|model| *model = progress);
    } else {
        NUTMEG_VIEW.update(|model| *model = progress);
        NUTMEG_VIEW.resume();
    }
}

impl nutmeg::Model for Progress {
    fn render(&mut self, _width: usize) -> String {
        match self {
            Progress::None => String::new(),
            Progress::Backup {
                filename,
                scanned_file_bytes,
                scanned_dirs,
                scanned_files,
                entries_new,
                entries_changed,
                entries_unchanged,
            } => format!("\
                Scanned {scanned_dirs} directories, {scanned_files} files, {} MB\n\
                {entries_new} new entries, {entries_changed} changed, {entries_unchanged} unchanged\n\
                {filename}",
                *scanned_file_bytes / 1_000_000,
            ),
            Progress::MeasureTree { files, total_bytes } => format!(
                "Measuring... {} files, {} MB",
                files,
                *total_bytes / 1_000_000
            ),
            Progress::Restore { filename, bytes_done }=>
        format!(
            "Restoring: {mb} MB\n{filename}",
            mb = *bytes_done / 1_000_000,
        ),
            Progress::ValidateBlocks {
                blocks_done,
                total_blocks,
                bytes_done,
                start,
            } => {
                format!(
                    "Check block {}/{}: {} done, {} MB checked, {} remaining",
                    blocks_done,
                    total_blocks,
                    nutmeg::percent_done(*blocks_done, *total_blocks),
                    *bytes_done / 1_000_000,
                    nutmeg::estimate_remaining(&start, *blocks_done, *total_blocks)
                )
            }
            Progress::ValidateBands {
                total_bands,
                bands_done,
                start,
            } => format!(
                "Check index {}/{}, {} done, {} remaining",
                bands_done,
                total_bands,
                nutmeg::percent_done(*bands_done, *total_bands),
                nutmeg::estimate_remaining(&start, *bands_done, *total_bands)
            ),
        }
    }
}
