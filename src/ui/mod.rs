// Conserve backup system.
// Copyright 2015, 2016, 2018 Martin Pool.

//! Abstract user interface trait.

use std::fmt;

pub use super::report::{Counts, Sizes};

use isatty;

pub mod color;
pub mod plain;

/// Display information about backup progress to the user in some way.
pub trait UI: fmt::Debug {
    /// Show counters, eg as a progress bar.
    fn show_progress(&mut self, counts: &Counts);

    /// Show a plain text message.
    fn print(&mut self, s: &str);

    /// Print an error message.
    fn problem(&mut self, s: &str);
}

/// Construct the best available UI for this environment.
///
/// This means: colored terminal if isatty etc, otherwise plain text.
pub fn best_ui() -> Box<UI + Send> {
    if !isatty::stdout_isatty() {
        Box::new(plain::PlainUI::new())
    } else if let Some(ui) = color::ColorUI::new() {
        Box::new(ui)
    } else {
        Box::new(plain::PlainUI::new())
    }
}

/// Construct a UI by name.
///
/// `ui_name` may be `"auto"`, `"plain"`, or `"color"`.
pub fn by_name(ui_name: &str) -> Option<Box<UI + Send>> {
    match ui_name {
        "auto" => Some(best_ui()),
        "plain" => Some(Box::new(plain::PlainUI::new())),
        "color" => match color::ColorUI::new() {
            Some(ui) => Some(Box::new(ui)),
            None => None,
        },
        _ => None,
    }
}

pub fn compression_percent(s: &Sizes) -> i64 {
    if s.uncompressed > 0 {
        100i64 - (100 * s.compressed / s.uncompressed) as i64
    } else {
        0
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
    use report::Sizes;

    #[test]
    pub fn best_ui() {
        // You can at least construct some kind of UI although the type depends on the environment.
        let _bestie = super::best_ui();
    }

    // TODO: Somehow test the type returned by `by_name`?
    #[test]
    pub fn by_name() {
        // You must get some UI back from the default.
        assert!(super::by_name("auto").is_some());
        // Invalid names give None.
        assert!(super::by_name("giraffe nativity").is_none());
        // Plain UI should always be possible.
        assert!(super::by_name("plain").is_some());
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
