// Copyright 2023-2026 Martin Pool

//! A monitor observes events from the library and can display them in a UI, log them, record them for testing, etc.

pub mod task;
pub mod test;
pub mod void;

use std::fmt::Debug;
use std::sync::Arc;

use self::task::Task;
use crate::{
    counters::{Counter, Counters},
    monitor::test::Collector,
};

/// A monitor receives events from the library and may collect them, report them
/// to the terminal, log them, etc.
///
/// The monitor can be cloned and passed between threads and all the clones will continue to send events
/// to the same implementation.
#[derive(Debug, Clone)]
pub struct Monitor(Arc<dyn MonitorImpl>);

impl Monitor {
    /// Make a new monitor with the given implementation.
    pub fn new(impl_: Arc<dyn MonitorImpl>) -> Self {
        Self(impl_)
    }

    /// Make a new void monitor that ignores all events.
    pub fn void() -> Self {
        Self::new(Arc::new(void::VoidMonitorImpl::new()))
    }

    /// Make a monitor for tests, and also return the [`Collector`] that records what happened during the tests.
    pub fn for_test() -> (Self, Arc<Collector>) {
        let collector = Collector::arc();
        (Self::new(collector.clone()), collector)
    }

    /// Notify that a counter increased by a given amount.
    pub fn count(&self, counter: Counter, increment: usize) {
        self.0.count(counter, increment);
    }

    /// Set the absolute value of a counter.
    pub fn set_counter(&self, counter: Counter, value: usize) {
        self.0.set_counter(counter, value);
    }

    pub fn get_counters(&self) -> Counters {
        self.0.get_counters()
    }

    /// A non-fatal error occurred.
    pub fn error(&self, error: crate::Error) {
        self.0.error(error);
    }

    /// Start a [`Task`] with the given name, and return a handle to it.
    pub fn start_task(&self, name: String) -> Task {
        self.0.start_task(name)
    }

    /// Emit some text output to stdout, followed by a newline.
    pub fn println(&self, text: &str) {
        self.0.println(text);
    }

    pub fn error_count(&self) -> usize {
        self.0.error_count()
    }
}

/// The specific implementation of a monitor, which receives events from the library.
///
/// The impl need not (and typically will not) be `Clone` because it's held by a [`Monitor`]
/// and is shared across threads.
pub trait MonitorImpl: Send + Sync + Debug + 'static {
    /// Notify that a counter increased by a given amount.
    fn count(&self, counter: Counter, increment: usize);

    /// Set the absolute value of a counter.
    fn set_counter(&self, counter: Counter, value: usize);

    fn get_counters(&self) -> Counters;

    /// A non-fatal error occurred.
    fn error(&self, error: crate::Error);

    fn start_task(&self, name: String) -> Task;

    fn println(&self, text: &str);

    fn error_count(&self) -> usize;
}
