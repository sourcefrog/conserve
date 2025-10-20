use crate::counters::Counter;

use super::{
    Monitor,
    task::{Task, TaskList},
};

/// A monitor that does not capture any information.
#[derive(Debug, Clone)]
pub struct VoidMonitor;
impl Monitor for VoidMonitor {
    fn count(&self, _counter: Counter, _increment: usize) {}

    fn set_counter(&self, _counter: Counter, _value: usize) {}

    fn error(&self, _error: crate::Error) {}

    fn start_task(&self, name: String) -> Task {
        /*
         * All data related to the target task will be dropped
         * as soon the callee drops the task.
         */
        let mut list = TaskList::default();
        list.start_task(name)
    }
}
