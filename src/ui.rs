// Conserve backup system.
// Copyright 2015, 2016, 2018, 2019, 2020 Martin Pool.

//! Abstract user interface trait.

use std::fmt::Write;
use std::io;
use std::io::Write as IoWrite;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use crossterm::{cursor, queue, style, terminal};
use lazy_static::lazy_static;
use thousands::Separable;
use unicode_segmentation::UnicodeSegmentation;

use crate::stats::Sizes;

const PROGRESS_RATE_LIMIT_MS: u32 = 200;

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
struct UIState {
    last_update: Option<Instant>,

    /// Is a progress bar currently on the screen?
    progress_present: bool,

    /// Should a progress bar be drawn?
    progress_enabled: bool,

    progress_state: ProgressState,
}

pub struct ProgressState {
    pub phase: String,
    start: Instant,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub filename: String,
}

lazy_static! {
    static ref UI_STATE: Mutex<UIState> = Mutex::new(UIState::default());
}

// TODO: Rather than a directly-called function, hook this into logging.
pub fn println(s: &str) {
    UI_STATE.lock().unwrap().println(s);
}

// TODO: Rather than a directly-called function, hook this into logging.
pub fn problem<S: AsRef<str>>(s: &S) {
    UI_STATE.lock().unwrap().problem(s.as_ref())
}

/// Report that a non-fatal error occurred.
///
/// The program will continue.
pub fn show_error(e: &dyn std::error::Error) {
    // TODO: Convert to logging.
    let mut buf = e.to_string();
    let mut cause = e;
    while let Some(c) = cause.source() {
        write!(&mut buf, "\n  caused by: {}", c).expect("Failed to format error cause");
        cause = c;
    }
    problem(&buf);
}

pub fn show_anyhow_error(e: &anyhow::Error) {
    // Debug format includes the cause and backtrace.
    // https://docs.rs/anyhow/1.0.31/anyhow/struct.Error.html#display-representations
    problem(&format!("{:?}", e))
}

pub fn set_progress_phase(s: &str) {
    let mut ui = UI_STATE.lock().unwrap();
    ui.progress_state.phase = s.to_string();
    ui.progress_state.bytes_done = 0;
    ui.show_progress();
}

pub fn set_progress_file(s: &str) {
    let mut ui = UI_STATE.lock().unwrap();
    ui.progress_state.filename = s.into();
    ui.show_progress();
}

pub fn set_bytes_total(bytes_total: u64) {
    UI_STATE.lock().unwrap().progress_state.bytes_total = bytes_total
}

pub fn increment_bytes_done(b: u64) {
    let mut ui = UI_STATE.lock().unwrap();
    ui.progress_state.bytes_done += b;
    ui.show_progress();
}

pub fn clear_progress() {
    let mut ui = UI_STATE.lock().unwrap();
    ui.clear_progress();
}

/// Enable drawing progress bars, only if stdout is a tty.
///
/// Progress bars are off by default.
pub fn enable_progress(enabled: bool) {
    use crossterm::tty::IsTty;
    let mut ui = UI_STATE.lock().unwrap();
    ui.progress_enabled = io::stdout().is_tty() && enabled;
}

impl Default for UIState {
    fn default() -> UIState {
        UIState {
            last_update: None,
            progress_present: false,
            progress_enabled: false,
            progress_state: ProgressState::default(),
        }
    }
}

impl Default for ProgressState {
    fn default() -> ProgressState {
        ProgressState {
            start: Instant::now(),
            phase: String::new(),
            filename: String::new(),
            bytes_done: 0,
            bytes_total: 0,
        }
    }
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
    /// Return false if it's too soon after the progress bar was last drawn.
    fn can_update_yet(&mut self) -> bool {
        if let Some(last) = self.last_update {
            let e = last.elapsed();
            e.as_secs() > 1 || e.subsec_millis() > PROGRESS_RATE_LIMIT_MS
        } else {
            true
        }
    }

    fn clear_progress(&mut self) {
        let mut stdout = io::stdout();
        if self.progress_present {
            queue!(
                stdout,
                terminal::Clear(terminal::ClearType::CurrentLine),
                cursor::MoveToColumn(0)
            )
            .unwrap();
            stdout.flush().unwrap();
            self.progress_present = false;
        }
        self.set_update_timestamp();
    }

    /// Remember that the ui was just updated, for the sake of throttling.
    fn set_update_timestamp(&mut self) {
        self.last_update = Some(Instant::now());
    }

    fn show_progress(&mut self) {
        if !self.progress_enabled || !self.can_update_yet() {
            return;
        }
        let w = if let Ok((w, _)) = terminal::size() {
            w as usize
        } else {
            return;
        };

        const SHOW_PERCENT: bool = true;

        let mut prefix = String::with_capacity(50);
        let mut message = String::with_capacity(200);
        let state = &self.progress_state;
        let elapsed = state.start.elapsed();
        let rate = mbps_rate(state.bytes_done, elapsed);
        if SHOW_PERCENT && state.bytes_total > 0 {
            write!(
                prefix,
                "{:>3}% ",
                100 * state.bytes_done / state.bytes_total
            )
            .unwrap();
        }
        write!(prefix, "{} ", duration_to_hms(elapsed)).unwrap();
        write!(
            prefix,
            "{:>15} ",
            crate::misc::bytes_to_human_mb(state.bytes_done),
        )
        .unwrap();
        write!(prefix, "{:>8} MB/s ", (rate as u64).separate_with_commas()).unwrap();
        write!(message, "{} {}", state.phase, state.filename).unwrap();
        let message_limit = w - prefix.len();
        let truncated_message = if message.len() < message_limit {
            message
        } else {
            UnicodeSegmentation::graphemes(message.as_str(), true)
                .take(message_limit)
                .collect::<String>()
        };
        let mut stdout = io::stdout();
        queue!(
            stdout,
            cursor::Hide,
            cursor::MoveToColumn(0),
            style::SetForegroundColor(style::Color::Green),
            style::Print(prefix),
            style::ResetColor,
            style::Print(truncated_message),
            terminal::Clear(terminal::ClearType::UntilNewLine),
            cursor::Show,
        )
        .unwrap();
        stdout.flush().unwrap();
        self.progress_present = true;
        self.set_update_timestamp();
    }

    fn println(&mut self, s: &str) {
        self.clear_progress();
        println!("{}", s);
    }

    fn problem(&mut self, s: &str) {
        self.clear_progress();
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
