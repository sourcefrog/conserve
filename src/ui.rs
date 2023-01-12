// Conserve backup system.
// Copyright 2015, 2016, 2018, 2019, 2020, 2021, 2022 Martin Pool.

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
use std::sync::Mutex;
use std::time::Duration;

use lazy_static::lazy_static;
use tracing::{info, warn};

use crate::stats::Sizes;
use crate::validate::{ValidateMonitor, ValidatePhase};
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

pub(crate) fn duration_to_hms(d: Duration) -> String {
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
        // Drawing this way makes messages leak from tests, for unclear reasons.

        // queue!(
        //     stdout,
        //     style::SetForegroundColor(style::Color::Red),
        //     style::SetAttribute(style::Attribute::Bold),
        //     style::Print("conserve error: "),
        //     style::SetAttribute(style::Attribute::Reset),
        //     style::Print(s),
        //     style::Print("\n"),
        //     style::ResetColor,
        // )
        // .unwrap();
    }
}

pub(crate) fn nutmeg_options() -> nutmeg::Options {
    nutmeg::Options::default().progress_enabled(UI_STATE.lock().unwrap().progress_enabled)
}

/// A ValidateMonitor that logs messages, collects problems in memory, optionally
/// writes problems to a json file, and draws console progress bars.
#[derive(Debug)]
pub struct TerminalValidateMonitor<JF>
where
    JF: io::Write + Debug,
{
    pub progress_bars: bool,
    /// Optionally write all problems as json to this file as they're discovered.
    pub problems_json: Option<Box<JF>>,
    pub log_problems: bool,
    pub n_problems: usize,
    pub log_phases: bool,
}

impl<JF> TerminalValidateMonitor<JF>
where
    JF: io::Write + Debug,
{
    pub fn new(problems_json: Option<JF>) -> Self {
        TerminalValidateMonitor {
            progress_bars: true,
            problems_json: problems_json.map(|x| Box::new(x)),
            log_problems: true,
            log_phases: true,
            n_problems: 0,
        }
    }

    pub fn saw_problems(&self) -> bool {
        self.n_problems > 0
    }
}

impl<JF> ValidateMonitor for TerminalValidateMonitor<JF>
where
    JF: io::Write + Debug,
{
    fn problem(&mut self, problem: Error) -> Result<()> {
        if self.log_problems {
            warn!("{problem}");
        }
        if let Some(f) = self.problems_json.as_mut() {
            // TODO: Structured serialization, not just a string.
            serde_json::to_writer_pretty(f, &problem.to_string())
                .map_err(|source| Error::SerializeProblem { source })?;
        }
        self.n_problems += 1;
        Ok(())
    }

    fn start_phase(&mut self, phase: ValidatePhase) {
        if self.log_phases {
            info!("{phase}");
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
