// Copyright 2023 Martin Pool

//! Communication from the library to a monitor: a test, a UI, etc.

pub mod collect;
pub mod messages;

use std::fmt::Debug;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

use crate::Apath;

pub trait Monitor {
    type TaskId: Clone + Debug + Eq + PartialEq + Hash + Send + Sync;

    /// Notify that a counter increased by a given amount.
    fn counter(&self, counter: Counter, increment: usize);

    /// Notify that a problem occurred.
    fn problem(&self, problem: Problem);

    /// Notify that a task has started.
    fn start_task(&self, task: Task) -> Self::TaskId;

    /// Notify that a task has made progress.
    ///
    /// Panics if the task id is not valid or has already stopped.
    fn update_task(&self, task_id: Self::TaskId, task: Task);

    /// Notify that a task has finished.
    fn stop_task(&self, task_id: Self::TaskId, task: Task);
}

#[derive(Debug)]
pub enum Problem {
    /// Some generic error.
    Error(crate::Error),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Counter {
    BlockRead,
    BlockWrite,
    BlockMatchExisting,
    BlockCacheHit,
    // ...
}

impl Counter {
    pub(self) const COUNT: usize = 4;
}

#[derive(Debug)]
pub enum Task {
    /// Overall backup
    Backup,
    /// Backup one file
    BackupFile { apath: Apath },
}

/// Track counters.
#[derive(Default)]
struct Counters {
    counters: [AtomicUsize; Counter::COUNT],
}

impl Counters {
    pub fn count(&self, counter: Counter, increment: usize) {
        self.counters[counter as usize].fetch_add(increment, Relaxed);
    }

    pub fn get(&self, counter: Counter) -> usize {
        self.counters[counter as usize].load(Relaxed)
    }
}
