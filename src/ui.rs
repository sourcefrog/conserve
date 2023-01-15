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

use std::fmt::{Debug, Write};
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use lazy_static::lazy_static;
use tracing::{error, info};

use crate::monitor::{Counters, Monitor, Phase, Progress};
use crate::stats::Sizes;
use crate::{Error, Result};

/// A terminal/text UI.
///
/// This manages interleaving log-type messages (info and error), interleaved
/// with progress bars.
///
/// Progress bars are only drawn when the application requests them with
/// `enable_progress` and the output destination is a tty that's capable
/// of redrawing.
///
/// So this class also works when stdout is redirected to a file, in
/// which case it will get only messages and no progress bar junk.
#[derive(Default)]
pub(crate) struct UIState {
    /// Should a progress bar be drawn?
    progress_enabled: bool,
}

lazy_static! {
    static ref UI_STATE: Mutex<UIState> = Mutex::new(UIState::default());
}

pub fn println(s: &str) {
    with_locked_ui(|ui| ui.println(s))
}

pub fn problem(s: &str) {
    with_locked_ui(|ui| ui.problem(s));
}

pub(crate) fn with_locked_ui<F>(mut cb: F)
where
    F: FnMut(&mut UIState),
{
    use std::ops::DerefMut;
    cb(UI_STATE.lock().unwrap().deref_mut())
}

pub(crate) fn format_error_causes(error: &dyn std::error::Error) -> String {
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
    let mut ui = UI_STATE.lock().unwrap();
    ui.progress_enabled = enabled;
}

#[allow(unused)]
pub(crate) fn compression_percent(s: &Sizes) -> i64 {
    if s.uncompressed > 0 {
        100i64 - (100 * s.compressed / s.uncompressed) as i64
    } else {
        0
    }
}

pub fn duration_to_hms(d: Duration) -> String {
    let elapsed_secs = d.as_secs();
    if elapsed_secs >= 3600 {
        format!(
            "{:2}:{:02}:{:02}",
            elapsed_secs / 3600,
            (elapsed_secs / 60) % 60,
            elapsed_secs % 60
        )
    } else {
        format!("   {:2}:{:02}", (elapsed_secs / 60) % 60, elapsed_secs % 60)
    }
}

#[allow(unused)]
pub(crate) fn mbps_rate(bytes: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs() as f64 + f64::from(elapsed.subsec_millis()) / 1000.0;
    if secs > 0.0 {
        bytes as f64 / secs / 1e6
    } else {
        0f64
    }
}

/// Describe the compression ratio: higher is better.
#[allow(unused)]
pub(crate) fn compression_ratio(s: &Sizes) -> f64 {
    if s.compressed > 0 {
        s.uncompressed as f64 / s.compressed as f64
    } else {
        0f64
    }
}

impl UIState {
    pub(crate) fn println(&mut self, s: &str) {
        // TODO: Go through Nutmeg instead...
        // self.clear_progress();
        println!("{s}");
    }

    fn problem(&mut self, s: &str) {
        // TODO: Go through Nutmeg instead...
        // self.clear_progress();
        println!("conserve error: {s}");
    }
}

pub(crate) fn nutmeg_options() -> nutmeg::Options {
    nutmeg::Options::default().progress_enabled(UI_STATE.lock().unwrap().progress_enabled)
}

/// A ValidateMonitor that logs messages, collects problems in memory, optionally
/// writes problems to a json file, and draws console progress bars.
pub struct TerminalValidateMonitor<JF>
where
    JF: io::Write + Debug + Send,
{
    /// Optionally write all problems as json to this file as they're discovered.
    pub problems_json: Mutex<Option<Box<JF>>>,
    /// Number of problems observed.
    n_problems: AtomicUsize,
    nutmeg_view: nutmeg::View<Progress>,
    counters: Counters,
}

impl<JF> TerminalValidateMonitor<JF>
where
    JF: io::Write + Debug + Send,
{
    pub fn new(problems_json: Option<JF>) -> Self {
        let nutmeg_view = nutmeg::View::new(Progress::None, nutmeg_options());
        TerminalValidateMonitor {
            problems_json: Mutex::new(problems_json.map(Box::new)),
            n_problems: 0.into(),
            nutmeg_view,
            counters: Counters::default(),
        }
    }

    pub fn saw_problems(&self) -> bool {
        self.n_problems.load(Ordering::Relaxed) > 0
    }
}

impl<JF> Monitor for TerminalValidateMonitor<JF>
where
    JF: io::Write + Debug + Send,
{
    fn problem(&self, err: Error) -> Result<()> {
        let problem_str = err.to_string();
        error!("{problem_str}");
        problem(&problem_str); // TODO: Unify with logging
        if let Some(f) = self.problems_json.lock().unwrap().as_mut() {
            // TODO: Structured serialization, not just a string.
            serde_json::to_writer_pretty(f, &problem_str)
                .map_err(|source| Error::SerializeProblem { source })?;
        }
        self.n_problems.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn start_phase(&mut self, phase: Phase) {
        info!("{phase}");
    }

    fn progress(&self, progress: Progress) {
        if matches!(progress, Progress::None) {
            self.nutmeg_view.suspend();
            self.nutmeg_view.update(|model| *model = progress);
        } else {
            self.nutmeg_view.update(|model| *model = progress);
            self.nutmeg_view.resume();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_compression_ratio() {
        let ratio = compression_ratio(&Sizes {
            compressed: 2000,
            uncompressed: 4000,
        });
        assert_eq!(format!("{ratio:3.1}x"), "2.0x");
    }
}
