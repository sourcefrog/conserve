// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Display log messages to stdout with no color or cursor movement,
//! perhaps for a log file.


use log;

/// Log in plain text to stdout.
pub struct TextLogger;

impl TextLogger {
    pub fn new() -> Option<TextLogger> {
        Some(TextLogger)
    }
}

impl log::Log for TextLogger {
    fn enabled(&self, _metadata: &log::LogMetadata) -> bool {
        true
    }

    fn log(&self, record: &log::LogRecord) {
        if ! self.enabled(record.metadata()) {
            return;
        }
        println!("{}", record.args());
    }
}
