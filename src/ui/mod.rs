// Conserve backup system.
// Copyright 2015, 2016, 2018, 2019 Martin Pool.

//! Abstract user interface trait.

use std::fmt;
use std::time::Duration;

use isatty;

pub use super::report::{Counts, Report, Sizes};

pub mod color;
pub mod plain;

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
            if let Some(ui) = color::ColorUI::new(progress_bar) {
                return Some(Box::new(ui));
            }
        }
        Some(Box::new(plain::PlainUI::new()))
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
