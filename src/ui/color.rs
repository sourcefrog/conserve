// Conserve backup system.
// Copyright 2015, 2016, 2017 Martin Pool.

//! Display progress and messages on a rich terminal with color
//! and cursor movement.
//!
//! This acts as a view and the Report is the model.

use std::fmt;
use std::io::prelude::*;
use std::time::{Duration, Instant};

use term;

use report::Counts;
use ui::UI;

const MB: u64 = 1_000_000;

pub struct ColorUI {
    t: Box<term::StdoutTerminal>,
    last_update: Option<Instant>,

    /// Is a progress bar currently on the screen?
    progress_present: bool,

    /// Should a progress bar be drawn?
    progress_enabled: bool,
}

impl ColorUI {
    /// Return a new ColorUI or None if there isn't a suitable terminal.
    pub fn new(progress_bar: bool) -> Option<ColorUI> {
        if let Some(t) = term::stdout() {
            Some(ColorUI {
                t,
                last_update: None,
                progress_present: false,
                progress_enabled: progress_bar,
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

    fn clear_progress(&mut self) {
        if self.progress_present {
            self.t.carriage_return().unwrap();
            self.t.delete_line().unwrap();
            self.progress_present = false;
        }
    }
}

fn duration_to_hms(d: Duration) -> String {
    let elapsed_secs = d.as_secs();
    if elapsed_secs >= 3600 {
        format!(
            "{:2}:{:02}:{:02}",
            elapsed_secs / 3600,
            (elapsed_secs / 60) % 60,
            elapsed_secs % 60
        )
    } else {
        format!("   {:2}:{:02}", (elapsed_secs / 60) % 60, elapsed_secs % 60)
    }
}

fn mbps_rate(bytes: u64, elapsed: Duration) -> f64 {
    let float_secs = elapsed.as_secs() as f64;
    if float_secs > 0.0 {
        bytes as f64 / float_secs / MB as f64
    } else {
        0f64
    }
}

impl UI for ColorUI {
    fn show_progress(&mut self, counts: &Counts) {
        if !self.progress_enabled {
            return;
        }
        if self.progress_present && self.throttle_updates() {
            return;
        }
        self.clear_progress();
        self.last_update = Some(Instant::now());
        self.progress_present = true;

        let t = &mut self.t;
        // TODO: Input size should really be the number of source bytes before
        // block deduplication.
        // Measure compression on body bytes.
        let block_sizes = counts.get_size("block");
        let block_comp_ratio = super::compression_ratio(&block_sizes);
        let elapsed = counts.elapsed_time();
        // TODO: Truncate to screen width (or draw on multiple lines with cursor-up)?
        // TODO: Rate limit etc.
        // TODO: Also show current filename.
        // TODO: Don't special-case for backups.
        t.fg(term::color::GREEN).unwrap();
        write!(t, " {} ", duration_to_hms(elapsed)).unwrap();
        let uncomp_mb_str = format!("{}MB", block_sizes.uncompressed / MB);
        let comp_mb_str = format!("{}MB", block_sizes.compressed / MB);
        let uncomp_rate = mbps_rate(block_sizes.uncompressed, elapsed);

        t.fg(term::color::GREEN).unwrap();
        write!(t, "{:8}", counts.get_count("file")).unwrap();
        t.fg(term::color::WHITE).unwrap();
        write!(t, " files").unwrap();
        t.fg(term::color::GREEN).unwrap();
        write!(t, "{:8}", counts.get_count("dir")).unwrap();
        t.fg(term::color::WHITE).unwrap();
        write!(t, " dirs").unwrap();
        t.fg(term::color::GREEN).unwrap();
        write!(
            t,
            " {:>9} => {:<9} {:2.1}x {:6.1}MB/s",
            uncomp_mb_str, comp_mb_str, block_comp_ratio, uncomp_rate,
        )
        .unwrap();
        t.fg(term::color::WHITE).unwrap();
        t.flush().unwrap();
    }

    fn print(&mut self, s: &str) {
        self.clear_progress();
        let t = &mut self.t;
        writeln!(t, "{}", s).unwrap();
        t.flush().unwrap();
    }

    fn problem(&mut self, s: &str) {
        self.clear_progress();
        let t = &mut self.t;
        t.fg(term::color::RED).unwrap();
        (write!(t, "{}: ", s)).unwrap();
        t.reset().unwrap();
        t.flush().unwrap();
    }

    fn finish(&mut self) {
        self.clear_progress();
    }
}

impl fmt::Debug for ColorUI {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ColorUI")
            .field("last_update", &self.last_update)
            .field("progress_present", &self.progress_present)
            .finish()
    }
}
