// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Log info, errors, etc to the console or a file.


use log;

use super::ui::terminal::ConsoleLogger;
use super::ui::text::TextLogger;


pub fn establish_a_logger() {
    let logger_box: Box<log::Log> = match ConsoleLogger::new() {
        Some(l) => Box::new(l),
        None => Box::new(TextLogger::new().unwrap())
    };
    log::set_logger(|max_log_level| {
        max_log_level.set(log::LogLevelFilter::Info);
        logger_box
    }).ok();
}
