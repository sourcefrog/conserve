// Copyright 2023 Martin Pool

//! Collect monitored information so that it can be inspected by tests.

use std::mem::take;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Mutex;

use super::{Counter, Counters, Monitor, Problem};

/// A monitor that collects information for later inspection.
///
/// Problems are collected in a vector.
///
/// Tasks are ignored.
///
/// Totals of counters are kept.
#[derive(Default)]
pub struct CollectMonitor {
    pub problems: Mutex<Vec<Problem>>,
    counters: Counters,
    next_task_id: AtomicUsize,
}

impl CollectMonitor {
    pub fn new() -> Self {
        CollectMonitor::default()
    }

    pub fn get_counter(&self, counter: Counter) -> usize {
        self.counters.get(counter)
    }

    pub fn take_problems(&self) -> Vec<Problem> {
        take(self.problems.lock().unwrap().as_mut())
    }
}

impl Monitor for CollectMonitor {
    type TaskId = usize;

    fn counter(&self, counter: Counter, increment: usize) {
        self.counters.count(counter, increment)
    }

    fn problem(&self, problem: Problem) {
        self.problems.lock().unwrap().push(problem);
    }

    fn start_task(&self, _task: super::Task) -> Self::TaskId {
        // TODO: Record tasks?
        self.next_task_id.fetch_add(1, Relaxed)
    }

    fn update_task(&self, _task_id: Self::TaskId, _task: super::Task) {
        // TODO: Record tasks?
    }

    fn stop_task(&self, _task_id: Self::TaskId, _task: super::Task) {
        // TODO: Record tasks?
    }
}
