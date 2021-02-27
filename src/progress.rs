// Conserve backup system.
// Copyright 2015, 2016, 2018, 2019, 2020, 2021 Martin Pool.

// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Progress bars.

use std::fmt::Write;
use std::time::{Duration, Instant};

use thousands::Separable;

use crate::ui::with_locked_ui;

const PROGRESS_RATE_LIMIT: Duration = Duration::from_millis(200);

/// A progress bar, created from the UI.
pub struct ProgressBar {
    phase: String,
    /// The filename currently being processed.
    filename: String,
    total_work: usize,
    work_done: usize,
    bytes_done: u64,
    bytes_total: u64,
    percent: Option<f64>,
    start: Instant,

    /// The time this bar was last drawn on the screen, if it ever was.
    last_drawn: Option<Instant>,
}

impl ProgressBar {
    pub fn new() -> ProgressBar {
        ProgressBar {
            phase: String::new(),
            filename: String::new(),
            total_work: 0,
            work_done: 0,
            bytes_done: 0,
            bytes_total: 0,
            percent: None,
            start: Instant::now(),
            last_drawn: None,
        }
    }

    pub fn set_phase(&mut self, phase: String) {
        self.phase = phase;
        self.maybe_redraw();
    }

    pub fn set_filename(&mut self, filename: String) {
        self.filename = filename;
        self.maybe_redraw();
    }

    pub fn set_total_work(&mut self, total_work: usize) {
        self.total_work = total_work
    }

    pub fn increment_work_done(&mut self, inc: usize) {
        self.set_work_done(self.work_done + inc)
    }

    pub fn set_work_done(&mut self, work_done: usize) {
        self.work_done = work_done;
        self.maybe_redraw();
    }

    pub fn set_bytes_done(&mut self, bytes: u64) {
        self.bytes_done = bytes;
        self.maybe_redraw();
    }

    pub fn set_bytes_total(&mut self, bytes: u64) {
        self.bytes_total = bytes;
        self.maybe_redraw();
    }

    pub fn increment_bytes_done(&mut self, bytes: u64) {
        self.set_bytes_done(self.bytes_done + bytes)
    }

    pub fn set_percent(&mut self, percent: f64) {
        self.percent = Some(percent);
        self.maybe_redraw();
    }

    pub fn set_fraction(&mut self, num: usize, div: usize) {
        if div > 0 {
            self.set_percent((100f64 * num as f64) / (div as f64));
        }
    }

    fn maybe_redraw(&mut self) {
        if let Some(last) = self.last_drawn {
            if last.elapsed() < PROGRESS_RATE_LIMIT {
                return;
            }
        }
        self.last_drawn = Some(Instant::now());
        with_locked_ui(|ui| ui.draw_progress_bar(self));
    }

    fn estimate_remaining(&self, percent_done: f64) -> Option<Duration> {
        const MIN_ESTIMATE_WINDOW: Duration = Duration::from_millis(500);
        const MIN_ESTIMATE_PERCENT: f64 = 1f64;
        if percent_done < MIN_ESTIMATE_PERCENT {
            return None;
        }
        let elapsed = Instant::now() - self.start;
        if elapsed < MIN_ESTIMATE_WINDOW {
            return None;
        }
        Some(elapsed.mul_f64((100f64 - percent_done) / percent_done))
    }

    pub(crate) fn render_prefix(&self) -> String {
        let mut prefix = String::with_capacity(50);
        if !self.phase.is_empty() {
            write!(prefix, "{} ", self.phase).unwrap();
        }
        if self.total_work > 0 {
            if self.work_done > 0 {
                write!(
                    prefix,
                    "{}/{} ",
                    self.work_done.separate_with_commas(),
                    self.total_work.separate_with_commas()
                )
                .unwrap();
            } else {
                write!(prefix, "{} ", self.total_work.separate_with_commas()).unwrap();
            }
        } else if self.work_done > 0 {
            write!(prefix, "{} ", self.work_done.separate_with_commas()).unwrap();
        }

        if self.bytes_done > 0 {
            write!(
                prefix,
                "{:>15} ",
                crate::misc::bytes_to_human_mb(self.bytes_done)
            )
            .unwrap();
        }
        prefix
    }

    fn percent(&self) -> Option<f64> {
        if self.percent.is_some() {
            self.percent
        } else if self.total_work > 0 && self.work_done > 0 {
            Some(100f64 * self.work_done as f64 / self.total_work as f64)
        } else if self.bytes_total > 0 && self.bytes_done > 0 {
            Some(100f64 * self.bytes_done as f64 / self.bytes_total as f64)
        } else {
            None
        }
    }

    pub(crate) fn render_completion(&self) -> String {
        let mut percent_str = String::with_capacity(20);
        if let Some(percent) = self.percent() {
            write!(percent_str, "{:>4.1}% ", percent).unwrap();
            if let Some(remaining) = self.estimate_remaining(percent) {
                write!(percent_str, "{} remaining ", duration_brief(remaining)).unwrap();
            }
        }
        percent_str
    }

    pub(crate) fn render_filename(&self) -> String {
        self.filename.clone()
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        with_locked_ui(|ui| ui.clear_progress())
    }
}

impl Default for ProgressBar {
    fn default() -> Self {
        ProgressBar::new()
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
