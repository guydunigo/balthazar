pub mod arguments;

use std::sync::{Arc, Mutex};

use self::arguments::Arguments;
use super::JobId;
use super::Pid;

// Arbitrary id given by job sender or hash ?
pub type TaskId = usize;
pub type TaskArcMut = Arc<Mutex<Task>>;

#[derive(Debug)]
pub struct LoneTask {
    pub job_id: JobId,
    pub task: Task,
}

// TODO: id with clone ?
#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub args: Arguments, // TODO: wasm arg list ?
    pub result: Option<Result<Arguments, ()>>,
    // TODO: Pid + timestamp
    is_available: bool,
    // TODO: date?
}

impl Task {
    pub fn new(id: TaskId, args: Arguments) -> Task {
        Task {
            id,
            args,
            result: None,
            is_available: true,
        }
    }

    pub fn is_available(&self) -> bool {
        self.result.is_none() && self.is_available
    }

    pub fn set_available(&mut self, _peer_pid: Pid) {
        self.is_available = true;
    }

    pub fn set_unavailable(&mut self, _peer_pid: Pid) {
        self.is_available = false;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert!(true);
    }
}
