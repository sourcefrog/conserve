// Conserve backup system.
// Copyright 2015, 2016, 2018, 2019, 2020 Martin Pool.

//! Abstract user interface trait.

use std::fmt::Write;
use std::io::Write as IoWrite;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use crossterm::{cursor, queue, style, terminal};
use lazy_static::lazy_static;
use thousands::Separable;
use unicode_segmentation::UnicodeSegmentation;

use crate::report::{Counts, Sizes};
use crate::Result;

const PROGRESS_RATE_LIMIT_MS: u32 = 200;

/// A terminal/text UI.
///
/// The same class is used whether or not we have a rich terminal,
/// or just plain text output. (For example, output is redirected
/// to a file, or the program's run with no tty.)
struct UIState {
    t: Box<std::io::Stdout>,

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

pub fn println(s: &str) {
    UI_STATE.lock().unwrap().println(s);
}

pub fn problem<S: AsRef<str>>(s: &S) {
    UI_STATE.lock().unwrap().problem(s.as_ref()).unwrap();
}

pub fn set_progress_phase<S: ToString>(s: &S) {
    let mut ui = UI_STATE.lock().unwrap();
    ui.progress_state.phase = s.to_string();
    ui.progress_state.bytes_done = 0;
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

impl Default for UIState {
    fn default() -> UIState {
        UIState {
            t: Box::new(std::io::stdout()),
            last_update: None,
            progress_present: false,
            progress_enabled: false,
            progress_state: ProgressState::default(),
        }
    }
}

impl ProgressState {
    pub fn from_counts(counts: &Counts) -> ProgressState {
        ProgressState {
            phase: counts.phase.clone(),
            start: counts.start,
            bytes_done: counts.total_work,
            bytes_total: counts.total_work,
            filename: counts.get_latest_filename().into(),
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
        if self.progress_present {
            #[allow(deprecated)]
            queue!(
                self.t,
                terminal::Clear(terminal::ClearType::CurrentLine),
                cursor::MoveToColumn(0)
            )
            .unwrap();
            self.t.flush().unwrap();
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
        // let block_sizes = counts.get_size("block");
        // let comp_bytes = block_sizes.compressed;
        // if comp_bytes > 0 {
        //     write!(
        //         pb_text,
        //         "=> {:<8} ",
        //         (block_sizes.compressed / MB).separate_with_commas(),
        //     )
        //     .unwrap();
        // }
        write!(prefix, "{:>8} MB/s ", (rate as u64).separate_with_commas(),).unwrap();
        write!(message, "{} {}", state.phase, state.filename).unwrap();
        let message_limit = w - prefix.len();
        let truncated_message = if message.len() < message_limit {
            message
        } else {
            UnicodeSegmentation::graphemes(message.as_str(), true)
                .take(message_limit)
                .collect::<String>()
        };
        #[allow(deprecated)]
        queue!(
            self.t,
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
        self.t.flush().unwrap();
        self.progress_present = true;
        self.set_update_timestamp();
    }

    fn println(&mut self, s: &str) {
        self.clear_progress();
        let t = &mut self.t;
        writeln!(t, "{}", s).unwrap();
        t.flush().unwrap();
    }

    fn problem(&mut self, s: &str) -> Result<()> {
        self.progress_present = false;
        // TODO: Only clear the line if progress bar is already present?
        #[allow(deprecated)]
        queue!(
            self.t,
            terminal::Clear(terminal::ClearType::CurrentLine),
            cursor::MoveToColumn(0),
            style::SetForegroundColor(style::Color::Red),
            style::SetAttribute(style::Attribute::Bold),
            style::Print("conserve error: "),
            style::SetAttribute(style::Attribute::Reset),
            style::Print(s),
            style::Print("\n"),
            style::ResetColor,
        )
        .unwrap();
        self.t.flush().expect("flush terminal output");
        Ok(())
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
