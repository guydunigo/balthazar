pub mod arguments;

use std::sync::{Arc, Mutex};

use self::arguments::Arguments;
use super::{JobId, PeerId, Hash256};

// Arbitrary id given by job sender or hash ?
pub type TaskId = Hash256;
pub type TaskArcMut = Arc<Mutex<Task>>;

// TODO: In result : PeerId + timestamp
pub type TaskResult=Result<Arguments, ()>;

#[derive(Debug)]
pub struct LoneTask {
    pub job_id: JobId,
    pub task: Task,
}

// TODO: Store id or just calculate it ?
// TODO: no `pub` ?
/// A Task represent a *request for computation*, it contains the arguments to be passed to the job.
/// Once executed, it will contain the result.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub args: Arguments, // TODO: wasm arg list ?
    // TODO: Vec<TaskResult> ?
    pub result: Option<TaskResult>,
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

    pub fn set_available(&mut self, _peer_pid: PeerId) {
        self.is_available = true;
    }

    pub fn set_unavailable(&mut self, _peer_pid: PeerId) {
        self.is_available = false;
    }

    // TODO: What if there is already a result ?
    pub fn add_result(&mut self, res: TaskResult) {
        self.result = Some(res);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert!(true);
    }
}
