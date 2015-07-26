// Conserve backup system.
// Copyright 2015 Martin Pool.

extern crate log;
extern crate term;

use log::{LogRecord, LogLevel, LogMetadata};

pub struct ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, _metadata: &LogMetadata) -> bool {
        true
    }

    fn log(&self, record: &LogRecord) {
        if ! self.enabled(record.metadata()) {
            return;
        }

        let mut t = term::stdout().unwrap();
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
