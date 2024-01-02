// Copyright 2023-2024 Martin Pool

//! Monitor on a terminal UI.

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::thread::{sleep, spawn, JoinHandle};
use std::time::Duration;

use nutmeg::{Destination, View};
use thousands::Separable;
use tracing::error;

use crate::counters::{Counter, Counters};
use crate::monitor::task::{Task, TaskList};
use crate::monitor::Monitor;
use crate::Error;

pub struct TermUiMonitor {
    // operation: Operation,
    counters: Arc<Counters>,
    // active_files: Mutex<Vec<String>>,
    tasks: Arc<Mutex<TaskList>>,
    view: Arc<View<Model>>,
    /// A thread that periodically updates the view's progress bars from the Model.
    ///
    /// This is None during drop when the thread has been joined, and if progress
    /// bars are disabled.
    poller: Option<JoinHandle<()>>,
    /// True to ask the poller thread to stop, during drop.
    stop_poller: Arc<AtomicBool>,
}

/// The nutmeg model.
pub(super) struct Model {
    counters: Arc<Counters>,
    tasks: Arc<Mutex<TaskList>>,
}

impl TermUiMonitor {
    /// Make a new terminal UI monitor.
    pub fn new(show_progress: bool) -> Self {
        let counters = Arc::new(Counters::default());
        let tasks = Arc::new(Mutex::new(TaskList::default()));
        // We'll update from a polling thread at regular intervals, so we don't need Nutmeg to rate limit updates.
        let options = nutmeg::Options::default()
            .update_interval(Duration::ZERO)
            .progress_enabled(show_progress)
            .destination(Destination::Stderr);
        let view = Arc::new(View::new(
            Model {
                counters: counters.clone(),
                tasks: tasks.clone(),
            },
            options,
        ));
        let stop_poller = Arc::new(AtomicBool::new(false));
        let poller = if show_progress {
            let view2 = view.clone();
            let stop_poller2 = stop_poller.clone();
            Some(spawn(move || {
                while !stop_poller2.load(Relaxed) {
                    view2.update(|_| {});
                    sleep(Duration::from_millis(100));
                }
            }))
        } else {
            None
        };
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

    pub fn clear_progress_bars(&self) {
        // TODO: Make Nutmeg understand when to clear stderr to write to stdout.
        self.view.clear();
    }

    pub fn counters(&self) -> &Counters {
        &self.counters
    }
}

impl Drop for TermUiMonitor {
    fn drop(&mut self) {
        self.stop_poller.store(true, Relaxed);
        if let Some(poller) = self.poller.take() {
            poller
                .join()
                .expect("Wait for nutmeg poller thread to stop");
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

    fn error(&self, error: Error) {
        error!(target: "conserve", "{error}");
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
                s += &format!("{:?}: {}\n", counter, value.separate_with_commas());
            }
        }
        for task in self.tasks.lock().unwrap().active_tasks() {
            s += &format!("{}\n", task);
        }
        s
    }
}
