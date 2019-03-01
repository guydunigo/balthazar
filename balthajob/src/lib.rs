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
use self::task::{TaskArcMut, TaskId};

// TODO: hash(pid + bytecode) or tuple(pid, hash(bytecode)) ?
pub type JobId = Hash256;
pub type JobArcMut = Arc<Mutex<Job>>;
pub type JobsMap = HashMap<JobId, JobArcMut>;

// TODO: wrapper to manage it, prevent overriding, ...
pub type JobsMapArcMut = Arc<Mutex<JobsMap>>;

// TODO: don't duplicate PeerId...
pub type PeerId = u32;
const PID_LEN: usize = 4;

pub enum Error {
    NoFreeTaskId,
}

// TODO: possibility to automatically assign result/merge tasks when tasks are duplicated ?
//      - ~~This implies a flag to set this to off~~ NEW IDEA: the user adds mute args to change the hash of the task
// TODO: Same comment for jobs (share or not share same code + all anyone to add tasks ?)
#[derive(Debug, Clone)]
pub struct Job {
    pub id: JobId,
    pub sender_pid: PeerId,
    pub bytecode: Vec<u8>,
    // TODO: beware of overriding, ...
    pub tasks: HashMap<TaskId, TaskArcMut>,
}

impl Job {
    // TODO: Take as well a hash given by the sender to compare ?
    pub fn new(sender_pid: PeerId, bytecode: Vec<u8>) -> Job {
        let id = calculate_job_id(sender_pid, &bytecode[..]);
        Job {
            id,
            sender_pid,
            bytecode,
            tasks: HashMap::new(),
        }
    }

    // TODO: check if id already exists : deal with collision
    pub fn add_task(&mut self, task: task::Task) {
        self.tasks.insert(task.id, Arc::new(Mutex::new(task)));
    }

    pub fn add_new_task_with_id(&mut self, task_id: TaskId, args: Arguments) {
        let task = task::Task::new(task_id, args);
        self.add_task(task);
    }

    fn get_free_task_id(&self) -> Result<TaskId, Error> {
        for i in 0..TaskId::max_value() {
            if self.tasks.get(&i).is_none() {
                return Ok(i);
            }
        }

        Err(Error::NoFreeTaskId)
    }

    pub fn add_new_task(&mut self, args: Arguments) -> Result<TaskId, Error> {
        let task_id = self.get_free_task_id()?;
        self.add_new_task_with_id(task_id, args);
        Ok(task_id)
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

// TODO: so if a sender sends two different jobs with the same bytecode, they will be stored in the same place.
//      - Is this a good thing ? (pros: no duplicate bytecode -> size + net and parse times)
//      - The job sender would have to track which task ids and results are for each job : easy, just write it in the arguments...
//      - OR : sender sends a uid with the job...
// TODO: other way to get the job_id : signature sent by sender?
fn calculate_job_id(sender_pid: PeerId, bytecode: &[u8]) -> JobId {
    let pid_bytes = sender_pid.to_le_bytes();
    let mut vec = Vec::with_capacity(bytecode.len() + PID_LEN);

    vec.extend_from_slice(&pid_bytes);
    vec.extend_from_slice(&bytecode);

    Hash256::hash(&vec[..])
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
