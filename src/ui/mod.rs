// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Abstract user interface trait.

pub use super::report::Counts;

use isatty;
use log;

pub mod color;
pub mod plain;


/// Display information about backup progress to the user in some way.
pub trait UI {
    /// Show counters, eg as a progress bar.
    fn show_progress(&mut self, counts: &Counts);

    /// Show a log message.
    fn log(&mut self, record: &log::LogRecord);
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

#[cfg(test)]
mod tests {
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
}
