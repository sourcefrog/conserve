// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Abstract user interface trait.

pub use super::report::Counts;

use log;

pub mod terminal;
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
    if let Some(ui) = terminal::TermUI::new() {
        Box::new(ui)
    } else {
        Box::new(plain::PlainUI::new())
    }
}
