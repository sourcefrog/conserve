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

use std::borrow::Cow;
use std::fmt::Write;
use std::sync::Mutex;
use std::time::Duration;

use lazy_static::lazy_static;

use crate::stats::Sizes;

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

// TODO: Rather than a directly-called function, hook this into logging.
pub fn println(s: &str) {
    with_locked_ui(|ui| ui.println(s))
}

// TODO: Rather than a directly-called function, hook this into logging.
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
        write!(&mut buf, "\n  caused by: {}", c).expect("Failed to format error cause");
        cause = c;
    }
    buf
}

/// Report that a non-fatal error occurred.
///
/// The program will continue.
pub fn show_error(e: &dyn std::error::Error) {
    // TODO: Convert to logging.
    problem(&format_error_causes(e));
}

/// Enable drawing progress bars, only if stdout is a tty.
///
/// Progress bars are off by default.
pub fn enable_progress(enabled: bool) {
    let mut ui = UI_STATE.lock().unwrap();
    ui.progress_enabled = enabled;
}

pub fn compression_percent(s: &Sizes) -> i64 {
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

fn duration_brief(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 120 {
        format!("{:4} min", secs / 60)
    } else {
        format!("{:4} sec", secs)
    }
}

pub fn mbps_rate(bytes: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs() as f64 + f64::from(elapsed.subsec_millis()) / 1000.0;
    if secs > 0.0 {
        bytes as f64 / secs / 1e6
    } else {
        0f64
    }
}

/// Describe the compression ratio: higher is better.
pub fn compression_ratio(s: &Sizes) -> f64 {
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
        println!("{}", s);
    }

    fn problem(&mut self, s: &str) {
        // TODO: Go through Nutmeg instead...
        // self.clear_progress();
        println!("conserve error: {}", s);
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

pub(crate) struct LinearModel {
    pub i: usize,
    pub n: usize,
    pub message: Cow<'static, str>,
}

impl LinearModel {
    pub(crate) fn new<S: Into<Cow<'static, str>>>(message: S, n: usize) -> LinearModel {
        LinearModel {
            i: 0,
            n,
            message: message.into(),
        }
    }
}

impl nutmeg::Model for LinearModel {
    fn render(&mut self, _width: usize) -> String {
        format!("{}: {}/{}", self.message, self.i, self.n)
    }
}

pub(crate) struct UnboundedModel {
    pub message: Cow<'static, str>,
    pub i: usize,
}

impl UnboundedModel {
    pub fn new<S: Into<Cow<'static, str>>>(message: S) -> UnboundedModel {
        UnboundedModel {
            i: 0,
            message: message.into(),
        }
    }
}

impl nutmeg::Model for UnboundedModel {
    fn render(&mut self, _width: usize) -> String {
        format!("{}: {}", self.message, self.i)
    }
}

pub(crate) fn nutmeg_options() -> nutmeg::Options {
    nutmeg::Options::default().progress_enabled(UI_STATE.lock().unwrap().progress_enabled)
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
        assert_eq!(format!("{:3.1}x", ratio), "2.0x");
    }
}
