// Conserve backup system.
// Copyright 2015, 2016, 2018, 2019, 2020 Martin Pool.

//! User interface: progress bar and messages.
//!
//! This acts as a view and the Report is the model.

use std::fmt;
use std::time::Duration;

use isatty;

use crate::report::{Report, Sizes};

use indicatif::{ProgressBar, ProgressStyle};

/// Display information about backup progress to the user in some way.
pub trait UI: fmt::Debug {
    /// Show counters, eg as a progress bar.
    fn show_progress(&mut self, report: &Report);

    /// Show a plain text message.
    fn print(&mut self, s: &str);

    /// Print an error message.
    fn problem(&mut self, s: &str);

    /// Clear up the UI before exiting.
    fn finish(&mut self);
}

impl dyn UI {
    /// Construct a UI by name.
    ///
    /// `ui_name` must be `"auto"`, `"plain"`, or `"color"`.
    pub fn by_name(ui_name: &str, progress_bar: bool) -> Option<Box<dyn UI + Send>> {
        if ui_name == "color" || (ui_name == "auto" && isatty::stdout_isatty()) {
            if let Some(ui) = ColorUI::new(progress_bar) {
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
pub struct ColorUI {
    pb: ProgressBar,
}

impl ColorUI {
    /// Return a new ColorUI or None if there isn't a suitable terminal.
    pub fn new(progress_bar: bool) -> Option<ColorUI> {
        const STYLE: &str =
            "[{elapsed_precise:8.green}] {bytes:>10.cyan} {bytes_per_sec:>10.cyan} {wide_msg}";
        let pb = if progress_bar {
            ProgressBar::new(0).with_style(ProgressStyle::default_bar().template(STYLE))
        } else {
            ProgressBar::hidden()
        };
        Some(ColorUI { pb })
    }
}

impl UI for ColorUI {
    fn show_progress(&mut self, report: &Report) {
        // TODO: Input size should really be the number of source bytes before
        // block deduplication.
        let counts = report.borrow_counts();
        // TODO: Synchronize the elapsed time in the report with the progress
        // bar?
        if counts.total_work > 0 {
            self.pb.set_length(counts.total_work);
        }
        self.pb.set_position(counts.done_work);
        self.pb.set_message(&format!(
            "{} {}",
            counts.phase,
            counts.get_latest_filename()
        ));
    }

    fn print(&mut self, s: &str) {
        self.pb.println(s);
    }

    fn problem(&mut self, s: &str) {
        // TODO: Draw with color even while coordinating with the
        // progress bar?
        self.pb.println(&format!("conserve error: {}", s));
    }

    fn finish(&mut self) {
        self.pb.finish_and_clear();
    }
}

impl fmt::Debug for ColorUI {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ColorUI").finish()
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

    fn print(&mut self, s: &str) {
        println!("{}", s);
    }

    fn problem(&mut self, s: &str) {
        self.print(s)
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
