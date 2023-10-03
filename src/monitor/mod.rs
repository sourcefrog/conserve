// Copyright 2023 Martin Pool

//! Communication from the library to a monitor: a test, a UI, etc.

pub mod collect;
pub mod task;

use std::fmt::Debug;

use self::task::Task;
use crate::counters::Counter;

pub trait Monitor: Send + Sync + 'static {
    /// Notify that a counter increased by a given amount.
    fn count(&self, counter: Counter, increment: usize);

    /// Set the absolute value of a counter.
    fn set_counter(&self, counter: Counter, value: usize);

    /// Notify that a problem occurred.
    fn problem(&self, problem: Problem);

    fn start_task(&self, name: String) -> Task;
}

#[derive(Debug)]
pub enum Problem {
    /// Some generic error.
    Error(crate::Error),
}
