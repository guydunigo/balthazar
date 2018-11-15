extern crate tokio;

extern crate balthajob as job;
extern crate balthernet as net;
extern crate balthmessage as message;
extern crate parity_wasm;
extern crate wasmi;

mod orchestrator;

//TODO: +everywhere stream or socket or ...

use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::convert::From;
use std::fmt::Display;
use std::fs::File;
use std::io;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::sync::Mutex;

use job::task::arguments::Arguments;
use job::task::{LoneTask, Task};
use job::Job;
use message::{de, Message, MessageReader};
use net::asynctest::shoal::{MpscReceiverMessage, ShoalReadArc};
use net::MANAGER_ID;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    FailedHandshake,
    IoError(io::Error),
    NetError(net::Error),
    MessageError(message::Error),
    DeserializeError(de::Error),
    UnexpectedReply(Message),
    NoReply,
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

impl From<message::Error> for Error {
    fn from(err: message::Error) -> Error {
        Error::MessageError(err)
    }
}

impl From<de::Error> for Error {
    fn from(err: de::Error) -> Error {
        Error::DeserializeError(err)
    }
}

// ------------------------------------------------------------------

pub fn fill<A: ToSocketAddrs + Display>(
    runtime: &mut Runtime,
    shoal: ShoalReadArc,
    shoal_rx: MpscReceiverMessage,
) -> Result<(), Error> {
    let pode_id = 0;
    {
        let mut f = File::open("main.wasm")?;
        let mut code: Vec<u8> = Vec::new();
        f.read_to_end(&mut code)?;

        shoal.lock().send_to(MANAGER_ID, Message::Job(0, code));
    }

    let shoal_rx_future = shoal_rx
        .fold(None, move |job_id, msg| {
            if let Some(job_id) = job_id {
                let mut f = File::open("args_list.ron").unwrap();
                let mut args_list_txt: Vec<u8> = Vec::new();
                f.read_to_end(&mut args_list_txt).unwrap();

                // Deserialize args_list
                let mut args_list: Vec<Arguments> = de::from_bytes(&args_list_txt[..]).unwrap();

                for args in args_list.drain(..) {
                    shoal
                        .lock()
                        .send_to(MANAGER_ID, Message::Task(job_id, 0, args));
                }
                // Then stop the receive loop:
                Err(())
            } else {
                match msg {
                    (_, Message::JobRegisteredAt(job_id)) => Ok(Some(job_id)),
                    _ => {
                        println!("Pode : {} : didn't receive job_id.", pode_id);
                        Ok(None)
                    }
                }
            }
        })
        .map(|_| ());

    runtime.spawn(shoal_rx_future);

    Ok(())
}

pub fn swim<A: ToSocketAddrs + Display>(
    runtime: &mut Runtime,
    shoal: ShoalReadArc,
    shoal_rx: MpscReceiverMessage,
) {
    let pode_id = 0;

    let mut lone_tasks: Vec<LoneTask<bool>> = Vec::new();

    let jobs: Vec<Arc<Mutex<Job<bool>>>> = Vec::new();
    let jobs = Arc::new(Mutex::new(jobs));

    let rx = orchestrator::start_orchestrator(shoal.clone(), pode_id, jobs.clone());
    let rx = Arc::new(Mutex::new(rx));

    let shoal_rx_future = shoal_rx
        .map_err(|_| net::Error::ShoalMpscError)
        .for_each(move |(_, msg)| match msg {
            Message::Job(job_id, bytecode) => {
                let new_lone_tasks = register_job(jobs.clone(), &mut lone_tasks, job_id, bytecode);
                lone_tasks = new_lone_tasks;
                rx.lock().unwrap().recv().unwrap();
                Ok(())
            }
            Message::Task(job_id, task_id, args) => {
                let res = register_task(
                    shoal.clone(),
                    pode_id,
                    jobs.clone(),
                    &mut lone_tasks,
                    job_id,
                    task_id,
                    args,
                );
                match res {
                    Ok(job_was_requested) => {
                        if !job_was_requested {
                            rx.lock().unwrap().recv().unwrap();
                        }
                        Ok(())
                    }
                    Err(err) => Err(net::Error::from(err)),
                }
            }
            Message::NoJob => {
                rx.lock().unwrap().recv().unwrap();
                Ok(())
            }
            _ => {
                /*{
                    let mut socket = socket.lock().unwrap();
                    Message::Disconnect.send(id, &mut *socket)
                }*/
                Ok(())
            }
        })
        .map(|_| ())
        .map_err(|_| ());

    runtime.spawn(shoal_rx_future);
}

fn register_job(
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job<bool>>>>>>,
    lone_tasks: &mut Vec<LoneTask<bool>>,
    job_id: usize,
    bytecode: Vec<u8>,
) -> Vec<LoneTask<bool>> {
    let mut new_lone_tasks = Vec::with_capacity(lone_tasks.len());

    // TODO: multiple jobs having same id ?
    // The use of `is_none` is due to `jobs` being borrowed...
    let job_opt = match jobs
        .lock()
        .unwrap()
        .iter()
        .find(|j| j.lock().unwrap().id == job_id)
    {
        Some(job) => Some(job.clone()),
        None => None,
    };

    match job_opt {
        // TODO: Useful ?
        Some(job) => job.lock().unwrap().set_bytecode(bytecode),
        None => {
            let mut job = Job::new(job_id, bytecode);

            for t in lone_tasks.drain(..) {
                if t.job_id == job_id {
                    job.push_task(t.task);
                } else {
                    new_lone_tasks.push(t);
                }
            }
            jobs.lock().unwrap().push(Arc::new(Mutex::new(job)));
        }
    }

    new_lone_tasks
}

fn register_task(
    shoal: ShoalReadArc,
    pode_id: usize,
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job<bool>>>>>>,
    lone_tasks: &mut Vec<LoneTask<bool>>,
    job_id: usize,
    task_id: usize,
    args: Arguments,
) -> Result<bool, message::Error> {
    //TODO: use balthajob to represent jobs and tasks and execute them there.
    //TODO: do not fail on job error
    let job_opt = match jobs
        .lock()
        .unwrap()
        .iter()
        .find(|j| j.lock().unwrap().id == job_id)
    {
        Some(job) => Some(job.clone()),
        None => None,
    };

    let job_was_requested = match job_opt {
        Some(job) => {
            job.lock().unwrap().push_new_task(task_id, args);
            false
        }
        None => {
            let task = Task::new(task_id, args);
            lone_tasks.push(LoneTask { job_id, task });
            shoal
                .lock()
                .send_to(MANAGER_ID, Message::RequestJob(job_id));

            true
        }
    };

    println!("{} : Task #{} for Job #{} saved.", pode_id, task_id, job_id);

    Ok(job_was_requested)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
