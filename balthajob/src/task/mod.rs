pub mod arguments;

use self::arguments::Arguments;
use super::JobId;

pub type TaskId = usize;

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

    pub fn set_available(&mut self) {
        self.is_available = true;
    }

    pub fn set_unavailable(&mut self) {
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
