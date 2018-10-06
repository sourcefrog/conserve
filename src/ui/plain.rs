// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Display log messages to stdout with no color or cursor movement,
//! perhaps for a log file.

use log;

use super::Counts;

use std::io;
use std::io::prelude::*;

#[derive(Debug, Default)]
pub struct PlainUI;

/// A plain text UI that prints log messages to stdout and does nothing about progress
/// counters.
impl PlainUI {
    /// Make a PlainUI.
    pub fn new() -> PlainUI {
        PlainUI {}
    }
}

impl super::UI for PlainUI {
    fn show_progress(&mut self, _counts: &Counts) {}

    /// Show a log message.
    fn log(&mut self, record: &log::LogRecord) {
        println!("{}", record.args());
    }

    fn print(&mut self, s: &str) {
        io::stdout().write_all(s.as_bytes()).unwrap();
    }
}
