extern crate tokio;

extern crate balthajob as job;
extern crate balthernet as net;
extern crate balthmessage as message;

use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::collections::HashMap;
use std::convert::From;
use std::io;
use std::sync::{Arc, Mutex};

use job::Job;
use job::JobsMapArcMut;
use message::Message;
use net::asynctest::shoal::{MpscReceiverMessage, PeerId, ShoalReadArc};

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    NetError(net::Error),
    ThreadPanicked,
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<net::Error> for Error {
    fn from(err: net::Error) -> Error {
        Error::NetError(err)
    }
}

// ------------------------------------------------------------------

pub fn swim(
    runtime: &mut Runtime,
    shoal: ShoalReadArc,
    shoal_rx: MpscReceiverMessage,
) -> Result<(), Error> {
    // TODO: hashmap in wrapper object
    let jobs_rc = Arc::new(Mutex::new(HashMap::new()));

    let shoal_rx_future = shoal_rx
        .map_err(|_| Error::NetError(net::Error::ShoalMpscError))
        .for_each(move |(peer_pid, msg)| {
            for_each_message(shoal.clone(), jobs_rc.clone(), peer_pid, msg)
        })
        .map(|_| ())
        .map_err(|_| ());

    runtime.spawn(shoal_rx_future);

    Ok(())
}

// ------------------------------------------------------------------

pub fn for_each_message(
    shoal: ShoalReadArc,
    jobs_rc: JobsMapArcMut,
    peer_pid: PeerId,
    msg: Message,
) -> Result<(), Error> {
    match msg {
        Message::Idle(i) => {
            for _ in 0..i {
                let mut jobs = jobs_rc.lock().unwrap();
                match job::get_available_task(&*jobs) {
                    Some((job, task)) => {
                        let mut task = task.lock().unwrap();
                        // If the sending fails, we don't register the task.
                        let job_id = job.lock().unwrap().id;
                        shoal
                            .lock()
                            .send_to(peer_pid, Message::Task(job_id, task.id, task.args.clone()));
                        task.set_unavailable(peer_pid);
                    }
                    None => {
                        shoal.lock().send_to(peer_pid, Message::NoJob);
                        break;
                    }
                }
            } // TODO: else send error ?
        }
        Message::RequestJob(job_id) => {
            let msg = match jobs_rc
                .lock()
                .unwrap()
                .iter()
                .find(|(id, _)| **id == job_id)
            {
                Some((job_id, job)) => {
                    // TODO: Don't like cloning probably big array...
                    let job = job.lock().unwrap();
                    Message::Job(job.sender_pid, *job_id, job.bytecode.clone())
                }
                None => Message::InvalidJobId(job_id),
            };

            shoal.lock().send_to(peer_pid, msg);
        }
        Message::Job(sender_pid, _job_id, job) => {
            // TODO: check job_id, send confirmation/error message ?
            let mut jobs = jobs_rc.lock().unwrap();

            let mut job = Job::new(sender_pid, job);

            jobs.insert(job.id, Arc::new(Mutex::new(job)));
        }
        // TODO: The pode who sends task should be the same as the one sending the job ?
        Message::Task(job_id, task_id, args) => {
            let mut jobs = jobs_rc.lock().unwrap();
            let job = match jobs.iter().find(|(id, _)| **id == job_id) {
                Some((_, job)) => job.clone(),
                None => return Ok(()), // TODO: Error unknown id.
            };

            job.lock().unwrap().add_new_task_with_id(task_id, args);
        }
        Message::ReturnValue(job_id, task_id, value) => {
            // TODO: deadlock?
            // TODO: check job and task ids ?
            let jobs = jobs_rc.lock().unwrap();
            let job = jobs.get(&job_id);
            if let Some(job) = job {
                let job = job.lock().unwrap();
                let task = job.tasks.get(&task_id);
                if let Some(task) = task {
                    let mut task = task.lock().unwrap();
                    task.result = Some(value);
                    task.set_unavailable(peer_pid);
                } else {
                    // TODO: proper error handling ?
                    eprintln!("Cephalo : {} : The pode sent a return value correpsonding to an unknown task, discarding...", peer_pid);
                }
            } else {
                // TODO: proper error handling ?
                eprintln!("Cephalo : {} : The pode sent a return value correpsonding to an unknown job, discarding...", peer_pid);
            }
        }
        _ => shoal
            .lock()
            .send_to(peer_pid, Message::Hello("Hey".to_string())),
    }

    Ok(())
}

// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
