//! Monitor on a terminal UI.

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn, JoinHandle};
use std::time::Duration;

use nutmeg::{Destination, View};

use crate::monitor::task::{Task, TaskList};
use crate::monitor::{counters::Counter, Counters, Monitor, Problem};

pub struct TermUiMonitor {
    // operation: Operation,
    counters: Arc<Counters>,
    // active_files: Mutex<Vec<String>>,
    tasks: Arc<Mutex<TaskList>>,
    view: Arc<View<Model>>,
    poller: Option<JoinHandle<()>>,
    stop_poller: Arc<AtomicBool>,
}

/// The nutmeg model.
pub(super) struct Model {
    counters: Arc<Counters>,
    tasks: Arc<Mutex<TaskList>>,
}

impl TermUiMonitor {
    pub fn new() -> Self {
        let counters = Arc::new(Counters::default());
        let tasks = Arc::new(Mutex::new(TaskList::default()));
        // We'll update from a polling thread at regular intervals, so we don't need Nutmeg to rate limit updates.
        let options = nutmeg::Options::default()
            .update_interval(Duration::ZERO)
            .destination(Destination::Stderr);
        let view = Arc::new(View::new(
            Model {
                counters: counters.clone(),
                tasks: tasks.clone(),
            },
            options,
        ));
        let stop_poller = Arc::new(AtomicBool::new(false));
        let view2 = view.clone();
        let stop_poller2 = stop_poller.clone();
        let poller = Some(spawn(move || {
            while !stop_poller2.load(Relaxed) {
                view2.update(|_| {});
                sleep(Duration::from_millis(100));
            }
        }));
        TermUiMonitor {
            counters,
            tasks,
            view,
            poller,
            stop_poller,
        }
    }

    pub(super) fn view(&self) -> Arc<View<Model>> {
        Arc::clone(&self.view)
    }
}

impl Default for TermUiMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TermUiMonitor {
    fn drop(&mut self) {
        self.stop_poller.store(true, Relaxed);
        self.poller
            .take()
            .expect("Poller thread should exist")
            .join()
            .expect("Wait for nutmeg poller thread to stop");
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
        self.view.message(format!("Problem: {:?}", problem));
    }

    fn start_task(&self, name: String) -> Task {
        self.tasks.lock().unwrap().start_task(name)
    }
}

impl nutmeg::Model for Model {
    fn render(&mut self, _width: usize) -> String {
        let mut s = String::new();
        for (counter, value) in self.counters.as_ref().iter() {
            if value > 0 {
                s += &format!("{:?}: {}\n", counter, value);
            }
        }
        for task in self.tasks.lock().unwrap().active_tasks() {
            s += &format!("{}\n", task);
        }
        s
    }
}
