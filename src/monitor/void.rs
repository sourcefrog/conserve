use std::sync::atomic::AtomicUsize;

use crate::counters::{Counter, Counters};

use super::{
    MonitorImpl,
    task::{Task, TaskList},
};

/// A monitor that does not capture any information aside from the error count and counters.
///
/// This is convenient for use in tests that don't care about checking for side effects:
/// but most tests actually should use [`conserve::monitor::Monitor::for_tests`] and make
/// assertions about the result.
#[derive(Debug, Default)]
pub struct VoidMonitorImpl {
    error_count: AtomicUsize,
    counters: Counters,
}

impl VoidMonitorImpl {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MonitorImpl for VoidMonitorImpl {
    fn count(&self, counter: Counter, increment: usize) {
        self.counters.count(counter, increment);
    }

    fn set_counter(&self, counter: Counter, value: usize) {
        self.counters.set(counter, value);
    }

    fn get_counters(&self) -> Counters {
        self.counters.clone()
    }

    fn error(&self, _error: crate::Error) {
        self.error_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn error_count(&self) -> usize {
        self.error_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn start_task(&self, name: String) -> Task {
        /*
         * All data related to the target task will be dropped
         * as soon the callee drops the task.
         */
        let mut list = TaskList::default();
        list.start_task(name)
    }

    fn println(&self, _text: &str) {}
}
