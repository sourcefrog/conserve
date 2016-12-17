// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

/// Generic UI trait.

use super::report::Counts;

use log;

pub mod terminal;
pub mod text;


pub trait UI {
    fn show_progress(&mut self, &Counts);

    /// Show a log message.
    fn log(&mut self, record: &log::LogRecord);
}
