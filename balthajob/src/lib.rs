pub mod task;

use std::sync::Arc;
use std::sync::Mutex;

// TODO: id with clone ?
#[derive(Debug, Clone)]
pub struct Job<T> {
    pub id: usize,
    pub bytecode: Vec<u8>,
    pub tasks: Vec<Arc<Mutex<task::Task<T>>>>,
    next_task_id: usize,
}

impl<T> Job<T> {
    pub fn new(id: usize, bytecode: Vec<u8>, mut tasks: Vec<task::Task<T>>) -> Job<T> {
        Job {
            id,
            bytecode,
            tasks: tasks.drain(..).map(|t| Arc::new(Mutex::new(t))).collect(),
            next_task_id: 0,
        }
    }

    pub fn get_free_job_id(list: &[Arc<Job<T>>]) -> Option<usize> {
        let mut id = 0;

        loop {
            if id >= usize::max_value() {
                break None;
            // TODO: not very efficient...
            } else if let Some(_) = list.iter().find(|job| job.id == id) {
                id += 1;
            } else {
                break Some(id);
            }
        }
    }

    fn get_new_task_id(&mut self) -> usize {
        let res = self.next_task_id;
        self.next_task_id += 1;
        res
    }

    pub fn new_task(&mut self, args: Vec<u8>) {
        let task = task::Task::new(self.get_new_task_id(), args);
        self.tasks.push(Arc::new(Mutex::new(task)));
    }

    pub fn get_available_task(&self) -> Option<Arc<Mutex<task::Task<T>>>> {
        match self.tasks.iter().find(|t| {
            let t = t.lock().unwrap();
            t.result.is_none() && match &t.pode {
                None => true,
                Some(p) => p.upgrade().is_none(),
            }
        }) {
            Some(t) => Some(t.clone()),
            None => None,
        }
    }
}

// TODO: name?
pub fn get_available_task<T>(
    jobs: &Vec<Arc<Job<T>>>,
) -> Option<(Arc<Job<T>>, Arc<Mutex<task::Task<T>>>)> {
    jobs.iter()
        .map(|job| (job, job.get_available_task()))
        .skip_while(|(_, task_option)| task_option.is_none())
        .map(|(job, task_option)| {
            if let Some(task) = task_option {
                (job.clone(), task.clone())
            } else {
                panic!("The task option shouldn't be None here.");
            }
        }).next()
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
