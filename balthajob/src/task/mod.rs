pub mod arguments;

use ron::ser;

use std::sync::{Arc, Mutex};

use self::arguments::Arguments;
use super::{Hash256, JobId, PeerId};

// Arbitrary id given by job sender or hash ?
pub type TaskId = Hash256;
pub type TaskArcMut = Arc<Mutex<Task>>;

// TODO: In result : PeerId + timestamp
pub type TaskResult = Result<Arguments, ()>;

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
    // TODO: Store id or just calculate it ?
    pub id: TaskId,
    pub args: Arguments, // TODO: wasm arg list ?
    // TODO: Vec<TaskResult> ?
    pub result: Option<TaskResult>,
    is_available: bool,
    // TODO: date?
    // TODO: owner/who is registered for result ?
}

impl Task {
    pub fn new(args: Arguments) -> Task {
        let id = calculate_task_id(&args);
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

// TODO: use job id
fn calculate_task_id(args: &Arguments) -> TaskId {
    // TODO: display problematic args
    let args_serialized = ser::to_string(args).expect("Couldn't serialize arguments.");
    Hash256::hash(args_serialized.as_bytes())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert!(true);
    }
}
