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

use crossterm::{cursor, queue, style, terminal};
use unicode_segmentation::UnicodeSegmentation;

use crate::ui::with_locked_ui;

/// A progress bar, created from the UI.
#[derive(Default)]
pub struct ProgressBar {
    phase: String,
    /// The filename currently being processed.
    filename: String,
    // TODO: Elapsed time.
    total_work: u64,
    work_done: u64,
    bytes_done: u64,
    bytes_total: u64,
    percent: Option<f64>,
    // TODO: Total work, done work, for percentages.
    // TODO: Total bytes, done bytes, for rate.
    // TODO: Maybe the UI should remember the progress bar state and redraw it
    // after printing a message or some other interruption? Or, maybe not, maybe
    // it's better to wait until there's another tick.
}

impl ProgressBar {
    pub fn set_phase(&mut self, phase: String) {
        self.phase = phase;
        self.maybe_redraw();
    }

    pub fn set_filename(&mut self, filename: String) {
        self.filename = filename;
        self.maybe_redraw();
    }

    pub fn set_total_work(&mut self, total_work: u64) {
        self.total_work = total_work
    }

    pub fn increment_work_done(&mut self, inc: u64) {
        self.set_work_done(self.work_done + inc)
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

    pub fn set_work_done(&mut self, work_done: u64) {
        self.work_done = work_done;
        self.maybe_redraw();
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

    fn maybe_redraw(&self) {
        with_locked_ui(|ui| ui.draw_progress_bar(self));
    }

    pub(crate) fn draw(&self, out: &mut dyn std::io::Write, width: usize) {
        let mut prefix = String::with_capacity(50);
        if !self.phase.is_empty() {
            write!(prefix, "{} ", self.phase).unwrap();
        }
        if self.total_work > 0 {
            if self.work_done > 0 {
                write!(prefix, "{}/{} ", self.work_done, self.total_work).unwrap();
            } else {
                write!(prefix, "{} ", self.total_work).unwrap();
            }
        }
        if self.bytes_done > 0 {
            write!(
                prefix,
                "{:>15} ",
                crate::misc::bytes_to_human_mb(self.bytes_done)
            )
            .unwrap();
        }

        let percent_str = if let Some(percent) = self.percent {
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
