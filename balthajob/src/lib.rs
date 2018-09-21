pub mod task;

// TODO: id with clone ?
#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub bytecode: Vec<u8>,
    pub tasks: Vec<task::Task>,
}

impl Job {
    pub fn new(id: usize, bytecode: Vec<u8>, tasks: Vec<task::Task>) -> Job {
        Job {
            id,
            bytecode,
            tasks,
        }
    }

    pub fn get_free_job_id(list: &[Job]) -> Option<usize> {
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
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
