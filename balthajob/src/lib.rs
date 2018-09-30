#![feature(int_to_from_bytes)]

// TODO: GENERAL: stop using usize as it might change between platforms.

#[macro_use]
extern crate serde_derive;

extern crate wasmi;

pub mod task;
pub mod wasm;

use std::sync::Arc;
use std::sync::Mutex;

type Arguments = task::arguments::Arguments;

// TODO: id with clone ?
#[derive(Debug, Clone)]
pub struct Job<T> {
    pub id: usize,
    pub bytecode: Vec<u8>,
    pub tasks: Vec<Arc<Mutex<task::Task<T>>>>,
    next_task_id: usize,
}

impl<T> Job<T> {
    pub fn new(id: usize, bytecode: Vec<u8>) -> Job<T> {
        Job {
            id,
            bytecode,
            tasks: Vec::new(),
            next_task_id: 0,
        }
    }

    pub fn get_free_job_id(list: &[Arc<Mutex<Job<T>>>]) -> Option<usize> {
        let mut id = 0;

        loop {
            if id >= usize::max_value() {
                break None;
            // TODO: not very efficient...
            } else if let Some(_) = list.iter().find(|job| job.lock().unwrap().id == id) {
                id += 1;
            } else {
                break Some(id);
            }
        }
    }

    fn get_new_task_id(&mut self) -> usize {
        let res = self.next_task_id;
        // TODO: usize limit ?
        self.next_task_id += 1;
        res
    }

    pub fn push_task(&mut self, task: task::Task<T>) {
        self.tasks.push(Arc::new(Mutex::new(task)));
    }

    pub fn push_new_task(&mut self, task_id: usize, args: Arguments) {
        let task = task::Task::new(task_id, args);
        self.push_task(task);
    }

    pub fn new_task(&mut self, args: Arguments) {
        let task_id = self.get_new_task_id();
        self.push_new_task(task_id, args);
    }

    // TODO: what happens between the mutex unlock and the new lock ? return MutexGuard ?
    pub fn get_available_task(&self) -> Option<Arc<Mutex<task::Task<T>>>> {
        match self.tasks.iter().find(|t| t.lock().unwrap().is_available()) {
            Some(t) => Some(t.clone()),
            None => None,
        }
    }

    pub fn set_bytecode(&mut self, bytecode: Vec<u8>) {
        self.bytecode = bytecode;
    }
}

// TODO: name?
// TODO: what happens between the mutex unlock and the new lock ? return MutexGuard ?
pub fn get_available_task<T>(
    jobs: &Vec<Arc<Mutex<Job<T>>>>,
) -> Option<(Arc<Mutex<Job<T>>>, Arc<Mutex<task::Task<T>>>)> {
    jobs.iter()
        .map(|job| (job, job.lock().unwrap().get_available_task()))
        .skip_while(|(_, task_option)| task_option.is_none())
        .map(|(job, task_option)| {
            if let Some(task) = task_option {
                (job.clone(), task.clone())
            } else {
                panic!("The task option shouldn't be None here.");
            }
        }).next()
}

/*
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
*/
