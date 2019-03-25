// TODO: GENERAL: stop using usize as it might change between platforms.

#[macro_use]
extern crate serde_derive;
extern crate sha3;
extern crate wasmi;

mod hash256;
pub mod task;
pub mod wasm;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use self::hash256::Hash256;
use self::task::arguments::Arguments;
use self::task::{Task, TaskArcMut, TaskId};

// TODO: hash(bytecode) or hash(pid + bytecode) or tuple(pid, hash(bytecode)) ?
pub type JobId = Hash256;
pub type JobArcMut = Arc<Mutex<Job>>;
pub type JobsMap = HashMap<JobId, JobArcMut>;

// TODO: wrapper to manage it, prevent overriding, ...
pub type JobsMapArcMut = Arc<Mutex<JobsMap>>;

// TODO: don't duplicate PeerId...
pub type PeerId = u32;

pub enum Error {
    NoFreeTaskId,
}

// TODO: possibility to automatically assign result/merge tasks when tasks are duplicated ?
//      - ~~This implies a flag to set this to off~~ NEW IDEA: the user adds mute args to change the hash of the task
// TODO: Same comment for jobs (share or not share same code + all anyone to add tasks ?)
/// A job represent the program's bytecode and its attached Tasks.
/// When it doesn't contain any Tasks, no computation will be performed.
/// TODO: describe the format of the bytecode
#[derive(Debug, Clone)]
pub struct Job {
    // TODO: Store id or just calculate it ?
    pub id: JobId,
    pub bytecode: Vec<u8>,
    // TODO: beware of overriding, ...
    pub tasks: HashMap<TaskId, TaskArcMut>,
}

impl Job {
    // TODO: Take as well a hash given by the sender to compare ?
    //          Or this is done via the p2p protocole ?
    pub fn new(bytecode: Vec<u8>) -> Job {
        let id = calculate_job_id(&bytecode[..]);
        Job {
            id,
            bytecode,
            tasks: HashMap::new(),
        }
    }

    // TODO: check if id already exists : deal with collision
    // TODO: Public ?
    pub fn add_task(&mut self, task: Task) {
        if self.tasks.contains_key(&task.id) {
            // TODO: better handling
            panic!("Importing key when it already exists");
        }
        self.tasks.insert(task.id, Arc::new(Mutex::new(task)));
    }

    pub fn add_new_task(&mut self, args: Arguments) -> TaskId {
        let task = Task::new(args);
        let task_id = task.id;
        self.add_task(task);
        task_id
    }

    // TODO: what happens between the mutex unlock and the new lock ? return MutexGuard ?
    pub fn get_available_task(&self) -> Option<TaskArcMut> {
        match self
            .tasks
            .iter()
            .find(|(_, t)| t.lock().unwrap().is_available())
        {
            Some((_, t)) => Some(t.clone()),
            None => None,
        }
    }

    pub fn set_bytecode(&mut self, bytecode: Vec<u8>) {
        self.bytecode = bytecode;
    }
}

// TODO: what happens between the mutex unlock and the new lock ? return MutexGuard ?
// TODO: get the peer id as an argument to choose which task to give, check that it doesn't have too many, ...
pub fn get_available_task(jobs: &JobsMap) -> Option<(JobArcMut, TaskArcMut)> {
    jobs.iter()
        .map(|(_, job)| (job, job.lock().unwrap().get_available_task()))
        .skip_while(|(_, task_option)| task_option.is_none())
        .map(|(job, task_option)| {
            if let Some(task) = task_option {
                (job.clone(), task.clone())
            } else {
                panic!("The task option shouldn't be None here.");
            }
        })
        .next()
}

fn calculate_job_id(bytecode: &[u8]) -> JobId {
    Hash256::hash(&bytecode[..])
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
