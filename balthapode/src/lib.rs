extern crate futures;
extern crate tokio;

extern crate balthajob as job;
extern crate balthernet as net;
extern crate balthmessage as message;
extern crate parity_wasm;
extern crate wasmi;

mod orchestrator;

use futures::sync::mpsc::Sender;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::collections::HashMap;
use std::convert::From;
use std::fs::File;
use std::io;
use std::process::exit;
use std::sync::{Arc, Mutex};

use job::task::arguments::Arguments;
use job::task::{LoneTask, Task, TaskId};
use job::{Job, JobId, JobsMapArcMut};
use message::{de, Message};
use net::asynctest::shoal::{MpscReceiverMessage, NotConnectedAction as NCA, ShoalReadArc};
use net::asynctest::PeerId;
use net::MANAGER_ID;

pub type PodeId = u64;

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

pub fn fill(
    runtime: &mut Runtime,
    shoal: ShoalReadArc,
    _shoal_rx: MpscReceiverMessage,
) -> Result<(), Error> {
    let code = {
        let mut f = File::open("main.wasm")?;
        let mut code: Vec<u8> = Vec::new();
        f.read_to_end(&mut code)?;
        code
    };

    let mut args_list: Vec<Arguments> = {
        let mut f = File::open("args_list.ron").unwrap();
        let mut args_list_txt: Vec<u8> = Vec::new();
        f.read_to_end(&mut args_list_txt).unwrap();

        // Deserialize args_list
        de::from_bytes(&args_list_txt[..]).unwrap()
    };
    let args_enumerated: Vec<(usize, Arguments)> = args_list.drain(..).enumerate().collect();

    // TODO: just put them all in a list and wait for all at once ?
    fn send_args(
        shoal: ShoalReadArc,
        peer_pid: PeerId,
        job_id: JobId,
        mut args_enumerated: Vec<(usize, Arguments)>,
    ) -> Box<Future<Item = (), Error = net::Error> + Send> {
        if let Some((task_id, args)) = args_enumerated.pop() {
            let shoal_clone = shoal.clone();
            let shoal_lock = shoal_clone.lock();
            Box::new(
                shoal_lock
                    .send_to_future_action(
                        peer_pid,
                        Message::Task(job_id, task_id, args),
                        NCA::Delay,
                    )
                    .and_then(move |_| send_args(shoal, peer_pid, job_id, args_enumerated)),
            )
        } else {
            exit(0);
        }
    }

    let job = Job::new(shoal.lock().local_pid(), code);
    let job_id = job.id;
    let shoal_clone = shoal.clone();

    let future = shoal
        .lock()
        .send_to_future_action(
            MANAGER_ID,
            Message::Job(shoal.lock().local_pid(), job_id, job.bytecode),
            NCA::Delay,
        )
        .and_then(move |_| {
            send_args(shoal_clone, MANAGER_ID, job_id, args_enumerated).map_err(net::Error::from)
        })
        .map(|_| ())
        .map_err(|_| ());

    runtime.spawn(future);

    Ok(())
}

pub fn swim(runtime: &mut Runtime, shoal: ShoalReadArc, shoal_rx: MpscReceiverMessage) {
    let pode_id = 0;

    let mut lone_tasks: Vec<LoneTask> = Vec::new();

    let jobs: HashMap<JobId, Arc<Mutex<Job>>> = HashMap::new();
    let jobs = Arc::new(Mutex::new(jobs));

    let tx = orchestrator::start_orchestrator(runtime, shoal.clone(), pode_id, jobs.clone());
    let tx = Arc::new(Mutex::new(tx));

    let shoal_rx_future = shoal_rx
        .map_err(|_| net::Error::ShoalMpscError)
        .for_each(move |(peer_pid, msg)| match msg {
            Message::Job(peer_id, job_id, bytecode) => {
                let new_lone_tasks = register_job(
                    peer_id,
                    jobs.clone(),
                    &mut lone_tasks,
                    tx.lock().unwrap().clone(),
                    job_id,
                    bytecode,
                );
                lone_tasks = new_lone_tasks;

                Ok(())
            }
            Message::Task(job_id, task_id, args) => register_task(
                shoal.clone(),
                pode_id,
                jobs.clone(),
                &mut lone_tasks,
                tx.lock().unwrap().clone(),
                job_id,
                task_id,
                args,
            ),
            Message::NoJob => Ok(()),
            _ => {
                println!(
                    "Pode : {} : Received a message but won't do anything.",
                    peer_pid
                );
                Ok(())
            }
        })
        .map(|_| ())
        .map_err(|_| ());

    runtime.spawn(shoal_rx_future);
}

fn register_job(
    peer_pid: PeerId,
    jobs: JobsMapArcMut,
    lone_tasks: &mut Vec<LoneTask>,
    tx: Sender<(JobId, TaskId)>,
    job_id: JobId,
    bytecode: Vec<u8>,
) -> Vec<LoneTask> {
    let mut new_lone_tasks = Vec::with_capacity(lone_tasks.len());

    // TODO: multiple jobs having same id ?
    let mut jobs_locked = jobs.lock().unwrap();

    match jobs_locked
        .iter()
        .find(|(id, _)| **id == job_id)
        .map(|(_, job)| job.clone())
    {
        // TODO: Useful ?
        Some(job) => job.lock().unwrap().set_bytecode(bytecode),
        None => {
            let mut job = Job::new(peer_pid, bytecode);
            let mut tasks_to_send = Vec::new();

            for t in lone_tasks.drain(..) {
                if t.job_id == job_id {
                    tasks_to_send.push(t.task.id);
                    job.add_task(t.task);
                } else {
                    new_lone_tasks.push(t);
                }
            }

            jobs_locked.insert(job.id, Arc::new(Mutex::new(job)));

            tasks_to_send.iter().for_each(|task_id| {
                tokio::spawn(
                    tx.clone()
                        .send((job_id, *task_id))
                        .map(|_| ())
                        .map_err(|err| {
                            eprintln!("Pode : Could not send task to executor : `{:?}`.", err);
                        }),
                );
            })
        }
    }

    new_lone_tasks
}

fn register_task(
    shoal: ShoalReadArc,
    pode_id: PodeId,
    jobs: JobsMapArcMut,
    lone_tasks: &mut Vec<LoneTask>,
    tx: Sender<(JobId, TaskId)>,
    job_id: JobId,
    task_id: TaskId,
    args: Arguments,
) -> Result<(), net::Error> {
    let jobs_locked = jobs.lock().unwrap();

    match jobs_locked.iter().find(|(id, _)| **id == job_id) {
        Some((job_id, job)) => {
            job.lock().unwrap().add_new_task_with_id(task_id, args);

            tokio::spawn(tx.send((*job_id, task_id)).map(|_| ()).map_err(|err| {
                eprintln!("Pode : Could not send task to executor : `{:?}`.", err);
            }));
        }
        None => {
            let task = Task::new(task_id, args);
            lone_tasks.push(LoneTask { job_id, task });
            shoal
                .lock()
                .send_to(MANAGER_ID, Message::RequestJob(job_id));
        }
    }

    println!("{} : Task #{} for Job #{} saved.", pode_id, task_id, job_id);

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
