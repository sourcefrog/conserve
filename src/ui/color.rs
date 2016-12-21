// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

//! Display progress and messages on a rich terminal with color
//! and cursor movement.
//!
//! This acts as a view and the Report is the model.

use std::io::prelude::*;
use std::time::{Duration, Instant};

use log;
use log::LogLevel;
use term;

use super::super::report::Counts;
use super::UI;

const MB: u64 = 1_000_000;

pub struct ColorUI {
    t: Box<term::StdoutTerminal>,
    last_update: Option<Instant>,
}


impl ColorUI {
    /// Return a new ColorUI or None if there isn't a suitable terminal.
    pub fn new() -> Option<ColorUI> {
        if let Some(t) = term::stdout() {
            Some(ColorUI{
                t: t,
                last_update: None,
            })
        } else {
            None
        }
    }

    fn throttle_updates(&mut self) -> bool {
        if let Some(last) = self.last_update {
            let e = last.elapsed();
            e.as_secs() < 1 && e.subsec_nanos() < 200_000_000
        } else {
            false
        }
    }
}


fn duration_to_hms(d: Duration) -> String {
    let elapsed_secs = d.as_secs();
    if elapsed_secs >= 3600 {
        format!("{:2}:{:02}:{:02}",
            elapsed_secs / 3600,
            (elapsed_secs / 60) % 60,
            elapsed_secs % 60)
    } else {
        format!("   {:2}:{:02}",
            (elapsed_secs / 60) % 60,
            elapsed_secs % 60)
    }
}


fn compression_percent(uncompressed_bytes: u64, compressed_bytes: u64) -> i64 {
    if uncompressed_bytes > 0 {
        100i64 - (100 * compressed_bytes / uncompressed_bytes) as i64
    } else {
        0
    }
}


fn mbps_rate(bytes: u64, elapsed: Duration) -> f64 {
    let float_secs = elapsed.as_secs() as f64
        + (elapsed.subsec_nanos() as f64 / 1e9);
    if float_secs > 0.0 {
        bytes as f64 / float_secs / MB as f64
    } else {
        0f64
    }
}


impl UI for ColorUI {
    fn show_progress(&mut self, counts: &Counts) {
        if self.throttle_updates() { return }
        self.last_update = Some(Instant::now());

        let mut t = &mut self.t;

        // t.delete_line().unwrap();
        // TODO: Input size should really be the number of source bytes before
        // block deduplication.
        // Measure compression on body bytes.
        let block_sizes = counts.get_size("block");
        let block_comp_pct = compression_percent(block_sizes.0, block_sizes.1);
        let elapsed = counts.elapsed_time();
        // TODO: Truncate to screen width (or draw on multiple lines with cursor-up)?
        // TODO: Rate limit etc.
        // TODO: Also show current filename.
        // TODO: Don't special-case for backups.
        t.fg(term::color::GREEN).unwrap();
        write!(t, " {} ", duration_to_hms(elapsed)).unwrap();
        let uncomp_mb_str = format!("{}MB", block_sizes.0 / MB);
        let comp_mb_str = format!("{}MB", block_sizes.1 / MB);
        let uncomp_rate = mbps_rate(block_sizes.0, elapsed);

        t.fg(term::color::GREEN).unwrap();
        write!(t, "{:8}", counts.get_count("file")).unwrap();
        t.fg(term::color::WHITE).unwrap();
        write!(t, " files").unwrap();
        t.fg(term::color::GREEN).unwrap();
        write!(t, "{:8}", counts.get_count("dir")).unwrap();
        t.fg(term::color::WHITE).unwrap();
        write!(t, " dirs").unwrap();
        t.fg(term::color::GREEN).unwrap();
        write!(t, " {:>9} => {:<9} {:3}% {:6.1}MB/s",
            uncomp_mb_str,
            comp_mb_str,
            block_comp_pct,
            uncomp_rate,
        ).unwrap();
        t.fg(term::color::WHITE).unwrap();
        t.carriage_return().unwrap();
        t.get_mut().flush().unwrap();
    }

    fn log(&mut self, record: &log::LogRecord) {
        let level = record.metadata().level();
        let mut t = &mut self.t;
        // TODO: Erase progress bar before logging and restore after.
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
