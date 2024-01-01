use crate::counters::Counter;

use super::{
    task::{Task, TaskList},
    Monitor, Problem,
};

/// A monitor that does not capture any information.
#[derive(Debug, Clone)]
pub struct VoidMonitor;
impl Monitor for VoidMonitor {
    fn count(&self, _counter: Counter, _increment: usize) {}

    fn set_counter(&self, _counter: Counter, _value: usize) {}

    fn problem(&self, _problem: Problem) {}

    fn start_task(&self, name: String) -> Task {
        /*
         * All data related to the target task will be dropped
         * as soon the callee drops the task.
         */
        let mut list = TaskList::default();
        list.start_task(name)
    }
}
