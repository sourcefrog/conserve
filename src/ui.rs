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
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use lazy_static::lazy_static;
#[allow(unused_imports)]
use tracing::{debug, error, info, trace, Level};

use crate::monitor::{Counters, Monitor, Progress};
use crate::{Error, Result};

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

pub fn enable_tracing(console_level: Level) {
    tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(console_level)
        .with_writer(WriteToNutmeg)
        .init();
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

pub(crate) fn format_error_causes(error: &dyn std::error::Error) -> String {
    use std::fmt::Write;
    let mut buf = error.to_string();
    let mut cause = error;
    while let Some(c) = cause.source() {
        write!(&mut buf, "\n  caused by: {c}").expect("Failed to format error cause");
        cause = c;
    }
    buf
}

/// Report that a non-fatal error occurred.
///
/// The program will continue.
pub fn show_error(e: &dyn std::error::Error) {
    // TODO: Log it.
    problem(&format_error_causes(e));
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

/// A ValidateMonitor that logs messages, collects problems in memory, optionally
/// writes problems to a json file, and draws console progress bars.
pub struct TerminalMonitor<JF>
where
    JF: io::Write + Debug + Send,
{
    /// Optionally write all problems as json to this file as they're discovered.
    pub problems_json: Mutex<Option<Box<JF>>>,
    /// Number of problems observed.
    n_problems: AtomicUsize,
    counters: Counters,
}

impl<JF> TerminalMonitor<JF>
where
    JF: io::Write + Debug + Send,
{
    pub fn new(problems_json: Option<JF>) -> Self {
        TerminalMonitor {
            problems_json: Mutex::new(problems_json.map(Box::new)),
            n_problems: 0.into(),
            counters: Counters::default(),
        }
    }

    pub fn saw_problems(&self) -> bool {
        self.n_problems.load(Ordering::Relaxed) > 0
    }
}

impl<JF> Monitor for TerminalMonitor<JF>
where
    JF: io::Write + Debug + Send,
{
    fn problem(&self, err: Error) -> Result<()> {
        let problem_str = err.to_string();
        error!("{problem_str}");
        if let Some(f) = self.problems_json.lock().unwrap().as_mut() {
            serde_json::to_writer_pretty(f, &err)
                .map_err(|source| Error::SerializeProblem { source })?;
        }
        self.n_problems.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn progress(&self, progress: Progress) {
        if matches!(progress, Progress::None) {
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
