extern crate futures;
extern crate tokio;

extern crate balthajob as job;
extern crate balthernet as net;
extern crate balthmessage as message;
extern crate parity_wasm;
extern crate wasmi;

mod orchestrator;

//TODO: +everywhere stream or socket or ...

use futures::sync::mpsc::Sender;
use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::timer::Delay;

use std::convert::From;
use std::fs::File;
use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use job::task::arguments::Arguments;
use job::task::TaskId;
use job::task::{LoneTask, Task};
use job::Job;
use job::JobId;
use message::{de, Message};
use net::asynctest::shoal::{MpscReceiverMessage, ShoalReadArc};
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
    shoal_rx: MpscReceiverMessage,
) -> Result<(), Error> {
    let pode_id = 0;
    {
        let shoal_clone = shoal.clone();
        let mut f = File::open("main.wasm")?;
        let mut code: Vec<u8> = Vec::new();
        f.read_to_end(&mut code)?;

        let delay_future = Delay::new(Instant::now() + Duration::from_secs(0))
            .map_err(net::Error::from)
            .and_then(move |_| {
                let shoal = shoal_clone.clone();
                let shoal = shoal.lock();
                shoal.send_to(MANAGER_ID, Message::Job(shoal.local_pid() as usize, code))
            })
            .map(|_| ())
            .map_err(|_| ());

        runtime.spawn(delay_future);
    }

    let shoal_rx_future = shoal_rx
        .for_each(move |(peer_pid, msg)| {
            match msg {
                Message::JobRegisteredAt(job_id) => {
                    let mut f = File::open("args_list.ron").unwrap();
                    let mut args_list_txt: Vec<u8> = Vec::new();
                    f.read_to_end(&mut args_list_txt).unwrap();

                    // Deserialize args_list
                    let mut args_list: Vec<Arguments> = de::from_bytes(&args_list_txt[..]).unwrap();

                    for args in args_list.drain(..) {
                        shoal
                            .lock()
                            .send_to(MANAGER_ID, Message::Task(job_id, 0, args))
                            .unwrap();
                    }

                    // Stop the client after all is sent
                    // TODO: use a more proper way (when all...)
                    let future = Delay::new(Instant::now() + Duration::from_secs(1))
                        .map(|_| std::process::exit(0))
                        .map_err(|_| ());
                    tokio::spawn(future);

                    // Then stop the receive loop:
                    Err(())
                }
                _ => {
                    println!(
                        "Pode : {} : didn't receive job_id from {}, received {:?}.",
                        pode_id, peer_pid, msg
                    );
                    Ok(())
                }
            }
        })
        .map(|_| ());

    runtime.spawn(shoal_rx_future);

    Ok(())
}

pub fn swim(runtime: &mut Runtime, shoal: ShoalReadArc, shoal_rx: MpscReceiverMessage) {
    let pode_id = 0;

    let mut lone_tasks: Vec<LoneTask> = Vec::new();

    let jobs: Vec<Arc<Mutex<Job>>> = Vec::new();
    let jobs = Arc::new(Mutex::new(jobs));

    let tx = orchestrator::start_orchestrator(runtime, shoal.clone(), pode_id, jobs.clone());
    let tx = Arc::new(Mutex::new(tx));

    let shoal_rx_future = shoal_rx
        .map_err(|_| net::Error::ShoalMpscError)
        .for_each(move |(peer_pid, msg)| match msg {
            Message::Job(job_id, bytecode) => {
                let new_lone_tasks = register_job(
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
                /*{
                    let mut socket = socket.lock().unwrap();
                    Message::Disconnect.send(id, &mut *socket)
                }*/
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
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job>>>>>,
    lone_tasks: &mut Vec<LoneTask>,
    tx: Sender<(JobId, TaskId)>,
    job_id: JobId,
    bytecode: Vec<u8>,
) -> Vec<LoneTask> {
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
            let mut tasks_to_send = Vec::new();

            for t in lone_tasks.drain(..) {
                if t.job_id == job_id {
                    tasks_to_send.push(t.task.id);
                    job.push_task(t.task);
                } else {
                    new_lone_tasks.push(t);
                }
            }

            jobs.lock().unwrap().push(Arc::new(Mutex::new(job)));
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
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job>>>>>,
    lone_tasks: &mut Vec<LoneTask>,
    tx: Sender<(usize, usize)>,
    job_id: JobId,
    task_id: TaskId,
    args: Arguments,
) -> Result<(), net::Error> {
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

    match job_opt {
        Some(job) => {
            job.lock().unwrap().push_new_task(task_id, args);

            tokio::spawn(tx.send((job_id, task_id)).map(|_| ()).map_err(|err| {
                eprintln!("Pode : Could not send task to executor : `{:?}`.", err);
            }));
        }
        None => {
            let task = Task::new(task_id, args);
            lone_tasks.push(LoneTask { job_id, task });
            shoal
                .lock()
                .send_to(MANAGER_ID, Message::RequestJob(job_id))?;
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
