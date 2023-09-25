//! Translate monitor operations to messages that can be queued.

use super::{Counter, Problem, Task};

#[derive(Debug)]
pub enum Message {
    Counter(Counter, usize),
    Problem(Problem),
    StartTask(Task),
    UpdateTask(Task),
    StopTask(Task),
}
