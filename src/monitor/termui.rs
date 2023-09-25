//! Monitor on a terminal UI.

use std::sync::Mutex;

use super::counters::Counters;
use super::{Counter, Monitor, Problem};

/// What high-level operation is being performed? This determines
/// how the progress is presented.
pub enum Operation {
    Backup,
    Restore,
    Validate,
}

pub struct TermUiMonitor {
    operation: Operation,
    counters: Counters,
    active_files: Mutex<Vec<String>>,
}

impl TermUiMonitor {
    pub fn new(operation: Operation) -> Self {
        TermUiMonitor {
            operation,
            counters: Counters::default(),
            active_files: Mutex::new(Vec::new()),
        }
    }
}

impl Monitor for TermUiMonitor {
    fn count(&self, counter: Counter, increment: usize) {
        self.counters.count(counter, increment)
    }

    fn set_counter(&self, counter: Counter, value: usize) {
        self.counters.set(counter, value)
    }

    fn problem(&self, problem: Problem) {
        // TODO: Through Nutmeg
        eprintln!("Problem: {:?}", problem);
    }

    fn start_file(&self, apath: &crate::Apath) {
        // TODO: Nutmeg
        let path = apath.to_string();
        println!("Start {:?}", path);
        let mut active_files = self.active_files.lock().unwrap();
        debug_assert!(!active_files.iter().any(|x| *x == path));
        active_files.push(path);
    }

    fn stop_file(&self, apath: &crate::Apath) {
        // TODO: Nutmeg
        let path = apath.to_string();
        println!("Finished {:?}", path);
        self.active_files.lock().unwrap().retain(|x| *x != path);
    }
}
