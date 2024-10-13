// Copyright 2023-2024 Martin Pool

//! Communication from the library to a monitor: a test, a UI, etc.

pub mod task;
pub mod test;

use self::task::Task;
use crate::counters::Counter;

/// A monitor receives events from the library and may collect them, report them
/// to the terminal, log them, etc.
pub trait Monitor: Send + Sync + 'static {
    /// Notify that a counter increased by a given amount.
    fn count(&self, counter: Counter, increment: usize);

    /// Set the absolute value of a counter.
    fn set_counter(&self, counter: Counter, value: usize);

    /// A non-fatal error occurred.
    fn error(&self, error: crate::Error);

    fn start_task(&self, name: String) -> Task;
}
