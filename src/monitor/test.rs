// Copyright 2023-2024 Martin Pool

//! Collect monitored information so that it can be inspected by tests.

use std::mem::take;
use std::sync::{Arc, Mutex};

use super::Monitor;
use super::task::{Task, TaskList};
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
pub struct TestMonitor {
    errors: Mutex<Vec<Error>>,
    counters: Counters,
    started_files: Mutex<Vec<Apath>>,
    task_list: Mutex<TaskList>,
}

impl TestMonitor {
    pub fn new() -> Self {
        TestMonitor::default()
    }

    /// Construct a new TestMonitor and wrap it in an Arc.
    pub fn arc() -> Arc<TestMonitor> {
        Arc::new(TestMonitor::new())
    }

    pub fn get_counter(&self, counter: Counter) -> usize {
        self.counters.get(counter)
    }

    /// Return the list of errors, and clear it.
    pub fn take_errors(&self) -> Vec<Error> {
        take(self.errors.lock().unwrap().as_mut())
    }

    /// Assert that no errors have yet occurred (since the list was cleared.)
    ///
    /// Panic if any errors have been reported.
    pub fn assert_no_errors(&self) {
        let errors = self.errors.lock().unwrap();
        assert!(errors.is_empty(), "Unexpected errors: {errors:#?}");
    }

    /// Assert the expected value of a counter.
    pub fn assert_counter(&self, counter: Counter, expected: usize) {
        let actual = self.counters.get(counter);
        assert_eq!(
            actual, expected,
            "Expected counter {counter:?} to be {expected}, but was {actual}",
        );
    }

    pub fn take_started_files(&self) -> Vec<Apath> {
        take(self.started_files.lock().unwrap().as_mut())
    }

    pub fn counters(&self) -> &Counters {
        &self.counters
    }
}

impl Monitor for TestMonitor {
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
