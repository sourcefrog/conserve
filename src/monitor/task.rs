// Copyright 2023 Martin Pool

//! Tasks are an abstraction to report progress on a long-running operation
//! from the core library to a UI, such as a progress bar.

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Weak};

#[derive(Default)]
pub struct TaskList {
    tasks: Vec<Weak<TaskInner>>,
}

impl TaskList {
    pub fn start_task(&mut self, name: String) -> Task {
        let inner = Arc::new(TaskInner {
            name,
            total: 0.into(),
            done: 0.into(),
        });
        self.tasks.push(Arc::downgrade(&inner));
        Task(inner)
    }

    pub fn active_tasks(&mut self) -> impl Iterator<Item = Arc<TaskInner>> {
        let mut v = Vec::new();
        self.tasks.retain(|task| {
            if let Some(inner) = task.upgrade() {
                v.push(inner);
                true
            } else {
                false
            }
        });
        v.into_iter()
    }
}

#[derive(Debug, Clone)]
/// A Task is constructed from a monitor. It can
/// be updated while it's alive. When it's dropped, the progress
/// bar is removed.
pub struct Task(Arc<TaskInner>);

impl Task {
    pub fn set_total(&self, total: usize) {
        self.0.total.store(total, Relaxed)
    }

    pub fn set_done(&self, done: usize) {
        self.0.done.store(done, Relaxed)
    }

    pub fn increment(&self, increment: usize) {
        self.0.done.fetch_add(1, Relaxed);
    }
}

#[derive(Debug)]
pub struct TaskInner {
    name: String, // TODO: Enum rather than string?
    total: AtomicUsize,
    done: AtomicUsize,
}
