// Conserve backup system.
// Copyright 2015, 2016, 2017, 2018 Martin Pool.

//! Display progress and messages on a rich terminal with color
//! and cursor movement.
//!
//! This acts as a view and the Report is the model.

use std::fmt;
use std::fmt::Write;
use std::time::Instant;

use term;
use terminal_size::{terminal_size, Width};
use thousands::Separable;
use unicode_segmentation::UnicodeSegmentation;

use crate::report::Report;
use crate::ui::{duration_to_hms, mbps_rate, UI};

const MB: u64 = 1_000_000;
const PROGRESS_RATE_LIMIT_MS: u32 = 100;

/// A terminal/text UI.
///
/// The same class is used whether or not we have a rich terminal,
/// or just plain text output. (For example, output is redirected
/// to a file, or the program's run with no tty.)
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
            e.as_secs() < 1 && e.subsec_millis() < PROGRESS_RATE_LIMIT_MS
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
        self.updated();
    }

    /// Remember that the ui was just updated, for the sake of throttling.
    fn updated(&mut self) {
        self.last_update = Some(Instant::now());
    }

    fn fg_color(&mut self, c: term::color::Color) {
        self.t.fg(c).unwrap();
    }

    fn reset_color(&mut self) {
        self.t.reset().unwrap();
    }
}

impl UI for ColorUI {
    fn show_progress(&mut self, report: &Report) {
        if !self.progress_enabled || self.throttle_updates() {
            return;
        }
        self.clear_progress();
        self.progress_present = true;

        let w = if let Some((Width(w), _)) = terminal_size() {
            w as usize
        } else {
            return;
        };

        // TODO: Input size should really be the number of source bytes before
        // block deduplication.
        let mut pb_text = String::with_capacity(200);
        {
            let counts = report.borrow_counts();
            let block_sizes = counts.get_size("block");
            let elapsed = counts.elapsed_time();
            let rate = mbps_rate(counts.done_work, elapsed);
            let comp_bytes = block_sizes.compressed;
            if counts.total_work > 0 {
                write!(
                    pb_text,
                    "{:>3}% ",
                    100 * counts.done_work / counts.total_work
                )
                .unwrap();
            };

            pb_text.push_str(&duration_to_hms(elapsed));

            write!(
                pb_text,
                "{:>8} MB ",
                (counts.done_work / MB).separate_with_commas(),
            )
            .unwrap();
            if comp_bytes > 0 {
                write!(
                    pb_text,
                    "=> {:<8} ",
                    (block_sizes.compressed / MB).separate_with_commas(),
                )
                .unwrap();
            }
            write!(
                pb_text,
                "{:6} MB/s  {} {}",
                (rate as u64).separate_with_commas(),
                counts.phase,
                counts.get_latest_filename()
            )
            .unwrap();
        };
        // TODO: If it's less than w bytes or characters, which will be a common
        // ascii case, we don't need to break graphemes.
        let g = UnicodeSegmentation::graphemes(pb_text.as_str(), true)
            .take(w)
            .collect::<String>();
        self.fg_color(term::color::GREEN);
        self.t.write_all(g.as_bytes()).unwrap();
        self.reset_color();
        self.t.flush().unwrap();
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
        t.fg(term::color::BRIGHT_RED).unwrap();
        t.attr(term::Attr::Bold).unwrap();
        (write!(t, "conserve error: ")).unwrap();
        t.reset().unwrap();
        (writeln!(t, "{}", s)).unwrap();
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
