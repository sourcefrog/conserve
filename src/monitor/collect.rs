// Copyright 2023-2024 Martin Pool

//! Collect monitored information so that it can be inspected by tests.

use std::mem::take;
use std::sync::{Arc, Mutex};

use super::task::{Task, TaskList};
use super::Monitor;
use crate::counters::{Counter, Counters};
use crate::{Apath, Error};

/// A monitor that collects information for later inspection,
/// particularly from tests.
///
/// Errors are collected in a vector.
///
/// Tasks are ignored.
///
/// Totals of counters are kept.
#[derive(Default)]
pub struct CollectMonitor {
    errors: Mutex<Vec<Error>>,
    counters: Counters,
    started_files: Mutex<Vec<Apath>>,
    task_list: Mutex<TaskList>,
}

impl CollectMonitor {
    pub fn new() -> Self {
        CollectMonitor::default()
    }

    pub fn get_counter(&self, counter: Counter) -> usize {
        self.counters.get(counter)
    }

    pub fn take_errors(&self) -> Vec<Error> {
        take(self.errors.lock().unwrap().as_mut())
    }

    pub fn take_started_files(&self) -> Vec<Apath> {
        take(self.started_files.lock().unwrap().as_mut())
    }

    pub fn arc() -> Arc<CollectMonitor> {
        Arc::new(CollectMonitor::new())
    }

    pub fn counters(&self) -> &Counters {
        &self.counters
    }
}

impl Monitor for CollectMonitor {
    fn count(&self, counter: Counter, increment: usize) {
        self.counters.count(counter, increment)
    }

    fn set_counter(&self, counter: Counter, value: usize) {
        self.counters.set(counter, value)
    }

    fn error(&self, error: Error) {
        self.errors.lock().unwrap().push(error);
    }

    fn start_task(&self, name: String) -> Task {
        self.task_list.lock().unwrap().start_task(name)
    }
}
