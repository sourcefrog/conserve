//! Monitor on a terminal UI.

use std::sync::{Arc, Mutex};

use nutmeg::View;
use strum::IntoEnumIterator;

use crate::monitor::{Counter, Counters, Monitor, Problem};

// /// What high-level operation is being performed? This determines
// /// how the progress is presented.
// pub enum Operation {
//     Backup,
//     Restore,
//     Validate,
// }

pub struct TermUiMonitor {
    // operation: Operation,
    counters: Arc<Counters>,
    active_files: Arc<Mutex<Vec<String>>>,
    view: View<Model>,
}

/// Internal state for Nutmeg.
struct Model {
    counters: Arc<Counters>,
    active_files: Arc<Mutex<Vec<String>>>,
}

impl TermUiMonitor {
    pub fn new() -> Self {
        // TODO: Hook up trace into this model.
        // TODO: View must be global (or leaked or just Arc?) to use as a trace target?
        let counters = Arc::new(Counters::default());
        let active_files = Arc::new(Mutex::new(Vec::new()));
        let view = View::new(
            Model {
                counters: counters.clone(),
                active_files: active_files.clone(),
            },
            nutmeg::Options::new().destination(nutmeg::Destination::Stderr),
        );
        TermUiMonitor {
            counters,
            active_files,
            view,
        }
    }
}

impl Default for TermUiMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl nutmeg::Model for Model {
    fn render(&mut self, _width: usize) -> String {
        let mut s = String::new();
        for i in Counter::iter() {
            let value = self.counters.get(i);
            if value > 0 {
                s.push_str(&format!("{:?}: {}\n", i, value));
            }
        }
        let active_files = self.active_files.lock().unwrap();
        for f in active_files.iter() {
            s.push_str(f);
            s.push('\n');
        }
        s
    }
}

impl Monitor for TermUiMonitor {
    fn count(&self, counter: Counter, increment: usize) {
        self.counters.count(counter, increment)
    }

    fn set_counter(&self, counter: Counter, value: usize) {
        self.counters.set(counter, value);
        self.view.update(|_| {})
    }

    fn problem(&self, problem: Problem) {
        // TODO: Through Nutmeg
        self.view.message(format!("Problem: {:?}", problem));
    }

    fn start_file(&self, apath: &crate::Apath) {
        // TODO: Nutmeg
        let path = apath.to_string();
        {
            let mut active_files = self.active_files.lock().unwrap();
            debug_assert!(!active_files.iter().any(|x| *x == path));
            active_files.push(path);
        }
        self.view.update(|_| {})
    }

    fn stop_file(&self, apath: &crate::Apath) {
        // TODO: Nutmeg
        let path = apath.to_string();
        println!("Finished {:?}", path);
        self.active_files.lock().unwrap().retain(|x| *x != path);
        self.view.update(|_| {})
    }
}
