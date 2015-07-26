// Copyright 2015 Martin Pool.

extern crate log;

use log::{LogRecord, LogLevel, LogMetadata};

pub struct ConsoleLogger;

impl log::Log for ConsoleLogger {
    fn enabled(&self, _metadata: &LogMetadata) -> bool {
        true
    }

    fn log(&self, record: &LogRecord) {
        let level_prefix = match record.metadata().level() {
            LogLevel::Error => "error: ",
            LogLevel::Warn => "warning: ",
            _ => "",
        };

        if self.enabled(record.metadata()) {
            println!("{}{}", level_prefix, record.args());
        }
    }
}
