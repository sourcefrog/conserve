// Conserve backup system.
// Copyright 2015, 2016, 2018 Martin Pool.

//! Display log messages to stdout with no color or cursor movement,
//! perhaps for a log file.

use super::Counts;

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
    fn show_progress(&mut self, _counts: &Counts) {}

    fn print(&mut self, s: &str) {
        println!("{}", s);
    }

    fn problem(&mut self, s: &str) {
        self.print(s)
    }

    fn finish(&mut self) {}
}
