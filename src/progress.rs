// Conserve backup system.
// Copyright 2015, 2016, 2018, 2019, 2020 Martin Pool.

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

use crossterm::{cursor, queue, style, terminal};
use thousands::Separable;
use unicode_segmentation::UnicodeSegmentation;

use crate::ui::with_locked_ui;

const PROGRESS_RATE_LIMIT: Duration = Duration::from_millis(200);

/// A progress bar, created from the UI.
#[derive(Default)]
pub struct ProgressBar {
    phase: String,
    /// The filename currently being processed.
    filename: String,
    // TODO: Elapsed time.
    total_work: usize,
    work_done: usize,
    bytes_done: u64,
    bytes_total: u64,
    percent: Option<f64>,

    /// The time this bar was last drawn on the screen, if it ever was.
    last_drawn: Option<Instant>,
    // TODO: Total bytes, done bytes, for rate.
}

impl ProgressBar {
    pub fn new() -> ProgressBar {
        ProgressBar::default()
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

    pub(crate) fn draw(&self, out: &mut dyn std::io::Write, width: usize) {
        let mut prefix = String::with_capacity(50);
        if !self.phase.is_empty() {
            write!(prefix, "{} ", self.phase).unwrap();
        }
        let mut work_percent = None;
        if self.total_work > 0 {
            if self.work_done > 0 {
                work_percent = Some(100f64 * self.work_done as f64 / self.total_work as f64);
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

        let percent = self.percent.or(work_percent);
        let percent_str = if let Some(percent) = percent {
            format!("{:>4.1}% ", percent)
        } else {
            String::new()
        };

        let mut message = String::with_capacity(200);
        if !self.filename.is_empty() {
            write!(message, "{}", self.filename).unwrap();
        }

        let message_limit = width - prefix.len() - percent_str.len();
        let truncated_message = if message.len() < message_limit {
            message
        } else {
            UnicodeSegmentation::graphemes(message.as_str(), true)
                .take(message_limit)
                .collect::<String>()
        };

        queue!(out, cursor::Hide, cursor::MoveToColumn(0),).unwrap();
        if !prefix.is_empty() {
            queue!(
                out,
                style::SetForegroundColor(style::Color::Green),
                style::Print(prefix),
            )
            .unwrap();
        }
        if !percent_str.is_empty() {
            queue!(
                out,
                style::SetForegroundColor(style::Color::Cyan),
                style::Print(percent_str),
            )
            .unwrap();
        }
        queue!(
            out,
            style::ResetColor,
            style::Print(truncated_message),
            terminal::Clear(terminal::ClearType::UntilNewLine),
            cursor::Show,
        )
        .unwrap();
        out.flush().unwrap();
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        with_locked_ui(|ui| ui.clear_progress())
    }
}
