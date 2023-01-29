// Conserve backup system.
// Copyright 2015-2023 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Console UI.

use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use lazy_static::lazy_static;
#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn, Level};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;

use crate::monitor::{Counters, Monitor, Progress};
use crate::Result;

/// Count of errors emitted to trace.
static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Count of warnings emitted to trace.
static WARN_COUNT: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    /// A global Nutmeg view.
    ///
    /// This is global to reflect that there is globally one stdout/stderr:
    /// this object manages it.
    static ref NUTMEG_VIEW: nutmeg::View<Progress> =
        nutmeg::View::new(Progress::None, nutmeg::Options::default()
            .destination(nutmeg::Destination::Stderr));
}

/// Return the number of errors logged in the program so far.
pub fn global_error_count() -> usize {
    ERROR_COUNT.load(Ordering::Relaxed)
}

/// Return the number of warnings logged in the program so far.
pub fn global_warn_count() -> usize {
    WARN_COUNT.load(Ordering::Relaxed)
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
            .with(CounterLayer())
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

/// A tracing Layer that counts errors and warnings into static counters.
struct CounterLayer();

impl<S> Layer<S> for CounterLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        match *event.metadata().level() {
            Level::ERROR => ERROR_COUNT.fetch_add(1, Ordering::Relaxed),
            Level::WARN => WARN_COUNT.fetch_add(1, Ordering::Relaxed),
            _ => 0,
        };
    }
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

pub fn println(s: &str) {
    // TODO: Reconsider callers; some should move to logging, others to a
    // new bulk output API?
    NUTMEG_VIEW.clear();
    println!("{s}");
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

/// A Monitor that draws progress bars on the terminal.
pub struct TerminalMonitor {
    counters: Counters,
}

impl TerminalMonitor {
    pub fn new() -> Result<Self> {
        Ok(TerminalMonitor {
            counters: Counters::default(),
        })
    }
}

impl Monitor for TerminalMonitor {
    fn progress(&self, progress: Progress) {
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

    fn counters(&self) -> &Counters {
        &self.counters
    }
}

impl nutmeg::Model for Progress {
    fn render(&mut self, _width: usize) -> String {
        match *self {
            Progress::None => String::new(),
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
                    nutmeg::percent_done(blocks_done, total_blocks),
                    bytes_done / 1_000_000,
                    nutmeg::estimate_remaining(&start, blocks_done, total_blocks)
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
                nutmeg::percent_done(bands_done, total_bands),
                nutmeg::estimate_remaining(&start, bands_done, total_bands)
            ),
        }
    }
}
