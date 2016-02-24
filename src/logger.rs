// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

extern crate log;
extern crate term;

use log::{LogRecord, LogLevel, LogLevelFilter, LogMetadata};
use std::sync::Mutex;


pub fn establish_a_logger() {
    let logger_box: Box<log::Log> = match ConsoleLogger::new() {
        Some(l) => Box::new(l),
        None => Box::new(TextLogger::new().unwrap())
    };
    log::set_logger(|max_log_level| {
        max_log_level.set(LogLevelFilter::Info);
        logger_box
    }).ok();
}


/// Log with colors to a terminal: only works if a real terminal is
/// available.
pub struct ConsoleLogger {
    term_mutex: Mutex<Box<term::StdoutTerminal>>
}

impl ConsoleLogger {
    /// Return a new ConsoleLogger if possible.
    ///
    /// Returns None if this process has no console.
    pub fn new() -> Option<ConsoleLogger> {
        term::stdout().and_then(|t| {
            Some(ConsoleLogger{
                term_mutex: Mutex::new(t)
            })})
    }
}

impl log::Log for ConsoleLogger {
    
    fn enabled(&self, _metadata: &LogMetadata) -> bool {
        true
    }

    fn log(&self, record: &LogRecord) {
        if ! self.enabled(record.metadata()) {
            return;
        }
        let mut t = self.term_mutex.lock().unwrap();
        let level = record.metadata().level();
        match level {
            LogLevel::Error | LogLevel::Warn => {
                t.fg(term::color::RED).unwrap();
                (write!(t, "{}: ", level)).unwrap();
                t.reset().unwrap();
            }
            _ => (),
        }
        writeln!(t, "{}", record.args()).unwrap();
    }
}


/// Log in plain text to stdout.
pub struct TextLogger;

impl TextLogger {
    pub fn new() -> Option<TextLogger> {
        Some(TextLogger)
    }
}

impl log::Log for TextLogger {
    fn enabled(&self, _metadata: &LogMetadata) -> bool {
        true
    }

    fn log(&self, record: &LogRecord) {
        if ! self.enabled(record.metadata()) {
            return;
        }
        println!("{}", record.args());
    }
}
