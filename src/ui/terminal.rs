// Conserve backup system.
// Copyright 2015, 2016 Martin Pool.

/// Display progress and messages on a terminal.
///
/// This acts as a view and the Report is the model.

use std::cell;
use std::io::prelude::*;

use term;

use super::super::report::Report;
use super::UI;


pub struct TermUI {
    t: cell::RefCell<Box<term::StdoutTerminal>>,
}


impl TermUI {
    /// Return a new TermUI or None if there isn't a suitable terminal.
    pub fn new() -> Option<TermUI> {
        if let Some(t) = term::stdout() {
            Some(TermUI{t: cell::RefCell::new(t)})
        } else {
            None
        }
    }
}


impl UI for TermUI {
    fn show_progress(&self, report: &Report) {
        const MB: u64 = 1000000;
        let mut t = self.t.borrow_mut();
        // t.delete_line().unwrap();
        // Measure compression on body bytes.
        let block_sizes = report.get_size("block.write");
        let block_comp_pct = if block_sizes.0 > 0 {
            100i64 - (100 * block_sizes.1 / block_sizes.0) as i64
        } else { 0 };
        let elapsed = report.elapsed_time();
        let elapsed_secs = elapsed.as_secs();
        let float_secs = elapsed.as_secs() as f64
            + (elapsed.subsec_nanos() as f64 / 1e9);
        let uncomp_rate = if float_secs > 0.0 {
            block_sizes.0 as f64 / float_secs / MB as f64
        } else {
            0f64
        };
        // TODO: Truncate to screen width (or draw on multiple lines with cursor-up)?
        // TODO: Rate limit etc.
        // TODO: Also show current filename.
        // TODO: Don't special-case for backups.
        t.fg(term::color::GREEN).unwrap();
        write!(t, "{:2}:{:02}:{:02} ",
            elapsed_secs / 3600,
            (elapsed_secs / 60) % 60,
            elapsed_secs % 60).unwrap();
        let uncomp_mb_str = format!("{}MB", block_sizes.0 / MB);
        let comp_mb_str = format!("{}MB", block_sizes.1 / MB);

        t.fg(term::color::GREEN).unwrap();
        write!(t, "{:8}", report.get_count("backup.file")).unwrap();
        t.fg(term::color::WHITE).unwrap();
        write!(t, " files").unwrap();
        t.fg(term::color::GREEN).unwrap();
        write!(t, "{:8}", report.get_count("backup.dir")).unwrap();
        t.fg(term::color::WHITE).unwrap();
        write!(t, " dirs").unwrap();
        t.fg(term::color::GREEN).unwrap();
        write!(t, " {:>9} => {:<9} {:3}% {:6.1}MB/s",
            uncomp_mb_str,
            comp_mb_str,
            block_comp_pct,
            uncomp_rate,
        ).unwrap();
        t.carriage_return().unwrap();
        t.get_mut().flush().unwrap();
    }
}
