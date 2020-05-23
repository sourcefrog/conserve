// Conserve backup system.
// Copyright 2015, 2016, 2018, 2019, 2020 Martin Pool.

//! Abstract user interface trait.

use std::fmt;
use std::fmt::Write;
use std::io::Write as IoWrite;
use std::time::Duration;
use std::time::Instant;

use crossterm::{cursor, queue, style, terminal};
use thousands::Separable;
use unicode_segmentation::UnicodeSegmentation;

use crate::report::{Report, Sizes};
use crate::Result;

const PROGRESS_RATE_LIMIT_MS: u32 = 200;

/// Display information about backup progress to the user in some way.
pub trait UI: fmt::Debug {
    /// Show counters, eg as a progress bar.
    fn show_progress(&mut self, report: &Report);

    /// Show a plain text message, followed by newline.
    fn println(&mut self, s: &str);

    /// Print an error message.
    fn problem(&mut self, s: &str) -> Result<()>;

    /// Clear up the UI before exiting.
    fn finish(&mut self);
}

impl dyn UI {
    /// Construct a UI by name.
    ///
    /// `ui_name` must be `"auto"`, `"plain"`, or `"color"`.
    pub fn by_name(ui_name: &str, progress_bar: bool) -> Option<Box<dyn UI + Send>> {
        if ui_name == "color" || (ui_name == "auto" && atty::is(atty::Stream::Stdout)) {
            if let Some(ui) = TerminalUI::new(progress_bar) {
                return Some(Box::new(ui));
            }
        }
        Some(Box::new(PlainUI::new()))
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

/// A terminal/text UI.
///
/// The same class is used whether or not we have a rich terminal,
/// or just plain text output. (For example, output is redirected
/// to a file, or the program's run with no tty.)
pub struct TerminalUI {
    t: Box<std::io::Stderr>,

    last_update: Option<Instant>,

    /// Is a progress bar currently on the screen?
    progress_present: bool,

    /// Should a progress bar be drawn?
    progress_enabled: bool,
}

impl TerminalUI {
    /// Return a new TerminalUI or None if there isn't a suitable terminal.
    pub fn new(progress_bar: bool) -> Option<TerminalUI> {
        Some(TerminalUI {
            t: Box::new(std::io::stderr()),
            last_update: None,
            progress_present: false,
            progress_enabled: progress_bar,
        })
    }

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
}

impl UI for TerminalUI {
    fn show_progress(&mut self, report: &Report) {
        if !self.progress_enabled || !self.can_update_yet() {
            return;
        }
        let w = if let Ok((w, _)) = terminal::size() {
            w as usize
        } else {
            return;
        };

        const SHOW_PERCENT: bool = true;

        // TODO: Input size should really be the number of source bytes before
        // block deduplication.
        let mut prefix = String::with_capacity(50);
        let mut message = String::with_capacity(200);
        {
            let counts = report.borrow_counts();
            let elapsed = counts.elapsed_time();
            let rate = mbps_rate(counts.done_work, elapsed);
            if SHOW_PERCENT && counts.total_work > 0 {
                write!(
                    prefix,
                    "{:>3}% ",
                    100 * counts.done_work / counts.total_work
                )
                .unwrap();
            }
            write!(prefix, "{} ", duration_to_hms(elapsed)).unwrap();
            write!(
                prefix,
                "{:>15} ",
                crate::misc::bytes_to_human_mb(counts.done_work),
            )
            .unwrap();
            write!(prefix, "{:>8} MB/s ", (rate as u64).separate_with_commas(),).unwrap();
            write!(message, "{} {}", counts.phase, counts.get_latest_filename()).unwrap();
        };
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

    fn finish(&mut self) {
        self.clear_progress();
    }
}

impl fmt::Debug for TerminalUI {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("TerminalUI")
            .field("last_update", &self.last_update)
            .field("progress_present", &self.progress_present)
            .finish()
    }
}

#[derive(Debug, Default)]
pub struct PlainUI;

/// A plain text UI that can be used when there is no terminal control.
///
/// Progress updates are just ignored.
impl PlainUI {
    /// Make a PlainUI.
    pub fn new() -> PlainUI {
        PlainUI {}
    }
}

impl super::UI for PlainUI {
    fn show_progress(&mut self, _report: &Report) {}

    fn println(&mut self, s: &str) {
        println!("{}", s);
    }

    fn problem(&mut self, s: &str) -> Result<()> {
        println!("conserve error: {}", s);
        Ok(())
    }

    fn finish(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::UI;
    use crate::report::Sizes;

    // TODO: Somehow test the type returned by `by_name`?
    #[test]
    pub fn by_name() {
        // You must get some UI back from the default.
        assert!(UI::by_name("auto", true).is_some());
        // Plain UI should always be possible.
        assert!(UI::by_name("plain", true).is_some());
    }

    #[test]
    pub fn test_compression_ratio() {
        let ratio = super::compression_ratio(&Sizes {
            compressed: 2000,
            uncompressed: 4000,
        });
        assert_eq!(format!("{:3.1}x", ratio), "2.0x");
    }
}
