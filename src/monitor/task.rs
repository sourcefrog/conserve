// Copyright 2023 Martin Pool

//! Tasks are an abstraction to report progress on a long-running operation
//! from the core library to a UI, such as a progress bar.

use std::fmt::Display;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, RwLock, Weak};

#[derive(Default)]
pub struct TaskList {
    tasks: Vec<Weak<TaskState>>,
}

impl TaskList {
    pub fn start_task(&mut self, name: String) -> Task {
        let inner = Arc::new(TaskState {
            name: name.into(),
            total: 0.into(),
            done: 0.into(),
        });
        self.tasks.push(Arc::downgrade(&inner));
        Task(inner)
    }

    pub fn active_tasks(&mut self) -> impl Iterator<Item = Arc<TaskState>> {
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
pub struct Task(Arc<TaskState>);

impl Task {
    pub fn set_total(&self, total: usize) {
        self.0.total.store(total, Relaxed)
    }

    pub fn set_done(&self, done: usize) {
        self.0.done.store(done, Relaxed)
    }

    pub fn increment(&self, increment: usize) {
        self.0.done.fetch_add(increment, Relaxed);
    }

    pub fn set_name(&self, name: String) {
        *self.0.name.write().unwrap() = name;
    }
}

impl AsRef<TaskState> for Task {
    fn as_ref(&self) -> &TaskState {
        self.0.as_ref()
    }
}

#[derive(Debug)]
pub struct TaskState {
    name: RwLock<String>,
    total: AtomicUsize,
    done: AtomicUsize,
}

impl TaskState {
    pub fn name(&self) -> String {
        self.name.read().unwrap().clone()
    }

    pub fn total(&self) -> usize {
        self.total.load(Relaxed)
    }

    pub fn done(&self) -> usize {
        self.done.load(Relaxed)
    }

    pub fn percent(&self) -> usize {
        let total = self.total.load(Relaxed);
        if total == 0 {
            0
        } else {
            self.done.load(Relaxed) * 100 / total
        }
    }
}

impl Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total = self.total.load(Relaxed);
        let done = self.done.load(Relaxed);
        let name = self.name.read().unwrap();
        if total == 0 && done == 0 {
            write!(f, "{}", name)
        } else if total == 0 {
            write!(f, "{}: {}", name, done)
        } else {
            write!(
                f,
                "{}: {}/{}, {:.1}%",
                name,
                done,
                total,
                done as f64 * 100.0 / total as f64
            )
        }
    }
}
