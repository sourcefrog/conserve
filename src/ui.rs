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
use std::fs::File;
use std::io::{self, BufWriter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use lazy_static::lazy_static;
#[allow(unused_imports)]
use tracing::{debug, error, info, trace, Level};

use crate::monitor::{Counters, Monitor, Progress};
use crate::{Error, Result};

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

lazy_static! {
    /// A global Nutmeg view.
    ///
    /// This is global to reflect that there is globally one stdout/stderr:
    /// this object manages it.
    // TODO: A const_default in Nutmeg, then this can be non-lazy.
    static ref NUTMEG_VIEW: nutmeg::View<Progress> =
        nutmeg::View::new(Progress::None, nutmeg::Options::default()
            .destination(nutmeg::Destination::Stderr));
}

/// Should progress be enabled for ad-hoc created Nutmeg views.
// (These should migrate to NUTMEG_VIEW.)
static PROGRESS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable_tracing(time_style: &TraceTimeStyle, console_level: Level) {
    use tracing_subscriber::fmt::time;
    let builder = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(console_level)
        .with_ansi(clicolors_control::colors_enabled())
        .with_writer(WriteToNutmeg);
    match time_style {
        TraceTimeStyle::None => builder.without_time().init(),
        TraceTimeStyle::Utc => builder.with_timer(time::UtcTime::rfc_3339()).init(),
        TraceTimeStyle::Relative => builder.with_timer(time::uptime()).init(),
        TraceTimeStyle::Local => builder
            .with_timer(time::OffsetTime::local_rfc_3339().unwrap())
            .init(),
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

pub fn println(s: &str) {
    // TODO: Reconsider callers; some should move to logging, others to a
    // new bulk output API?
    NUTMEG_VIEW.clear();
    println!("{s}");
}

pub fn problem(s: &str) {
    // TODO: Migrate callers to logging or to Monitor::problem.
    NUTMEG_VIEW.clear();
    println!("conserve error: {s}\n");
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

/// A ValidateMonitor that logs messages, draws to the ternminal, and optionally
/// writes errors to a json file.
pub struct TerminalMonitor {
    /// Optionally write all errors and warnings as json to this file as they're discovered.
    pub errors_json: Mutex<Option<BufWriter<File>>>,
    /// Number of errors observed.
    n_errors: AtomicUsize,
    counters: Counters,
}

impl TerminalMonitor {
    pub fn new(errors_json_path: Option<&PathBuf>) -> Result<Self> {
        let errors_json = Mutex::new(
            errors_json_path
                .map(File::create)
                .transpose()?
                .map(BufWriter::new),
        );
        Ok(TerminalMonitor {
            errors_json,
            n_errors: 0.into(),
            counters: Counters::default(),
        })
    }
}

impl Monitor for TerminalMonitor {
    fn error(&self, err: Error) -> Result<()> {
        error!("{err}");
        if let Some(f) = self.errors_json.lock().unwrap().as_mut() {
            serde_json::to_writer_pretty(f, &err)
                .map_err(|source| Error::SerializeError { source })?;
        }
        self.n_errors.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

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

    fn had_errors(&self) -> bool {
        self.n_errors.load(Ordering::Relaxed) > 0
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
