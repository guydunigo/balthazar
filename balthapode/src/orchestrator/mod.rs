use futures::future;
use futures::sync::mpsc::{self, Sender};
use tokio::prelude::*;
use tokio::runtime;
use tokio::runtime::Runtime;
use tokio::timer::{Delay, Interval};

use std::boxed::Box;
use std::io;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::PodeId;
use job;
use job::task::TaskId;
use job::wasm;
use job::Job;
use job::JobId;
use message;
use message::Message;
use net;
use net::asynctest::shoal::ShoalReadArc;
use net::asynctest::Pid;
use net::MANAGER_ID;

const NB_TASKS: usize = 1;
// TODO: good size ?
const CHANNEL_LIMIT: usize = 64;
const IDLE_CHECK_INTERVAL: u64 = 3;

// ------------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    WasmError(wasm::Error),
    MessageError(message::Error),
    SendMsgError(net::Error),
    ExecutorMpscError,
    JobNotFound(JobId),
    TaskNotFound(JobId, TaskId),
    ToShoalMpscError(mpsc::SendError<(Pid, Message)>),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<wasm::Error> for Error {
    fn from(err: wasm::Error) -> Error {
        Error::WasmError(err)
    }
}

impl From<message::Error> for Error {
    fn from(err: message::Error) -> Error {
        Error::MessageError(err)
    }
}

impl From<net::Error> for Error {
    fn from(err: net::Error) -> Error {
        Error::SendMsgError(err)
    }
}

// ------------------------------------------------------------------

// TODO: rename ?
pub fn start_orchestrator(
    runtime: &mut Runtime,
    shoal: ShoalReadArc,
    pode_id: PodeId,
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job>>>>>,
) -> Sender<(JobId, TaskId)> {
    let (send_msg_tx, send_msg_rx) = mpsc::channel(CHANNEL_LIMIT);
    let (tasks_tx, tasks_rx) = mpsc::channel(CHANNEL_LIMIT);
    let jobs_clone = jobs.clone();
    let shoal_clone = shoal.clone();

    thread::spawn(move || {
        // The wasm interpreter takes time to initialize, so we cache the "instance" for later use :
        let job_instances: Vec<(usize, wasm::ModuleRef)> = Vec::new();
        // Use Rc ?
        let job_instances = Arc::new(Mutex::new(job_instances));

        let future = tasks_rx
            .map_err(|_| Error::ExecutorMpscError)
            .for_each(move |(job_id, task_id)| {
                orchestrate(
                    pode_id,
                    jobs.clone(),
                    job_instances.clone(),
                    job_id,
                    task_id,
                    send_msg_tx.clone(),
                )
            })
            .map_err(move |err| {
                eprintln!("Executor : {} : Error : `{:?}`", pode_id, err);
            });
        let future = Delay::new(Instant::now() + Duration::from_secs(0))
            .map_err(|_| ())
            .and_then(|_| future);

        // TODO: using the same runtime as the rest ? or at least a multithreaded one to run tasks ?
        let mut runtime = runtime::current_thread::Runtime::new().unwrap();
        runtime.spawn(future);
        runtime.run().unwrap();
    });

    // TODO: Beware not to constantly lock jobs...
    // TODO: do not send idle if working on a job...
    let idle_watcher_future = Interval::new(
        Instant::now() + Duration::from_secs(5),
        Duration::from_secs(IDLE_CHECK_INTERVAL),
    )
    .map_err(|err| net::Error::TokioTimerError(err))
    .for_each(move |_| {
        let shoal = shoal_clone.clone();
        let jobs = jobs_clone.clone();

        let jobs = jobs.lock().unwrap();
        match job::get_available_task(&jobs[..]) {
            Some(_) => Ok(()),
            None => shoal.lock().send_to(MANAGER_ID, Message::Idle(NB_TASKS)),
        }
    })
    .map_err(|err| {
        eprintln!("Error in idle_watcher interval : {:?}", err);
    });
    runtime.spawn(idle_watcher_future);

    // This channel is needed as the executor is not running on the same tokio runtime as the shoal.
    // (The socket can't be registered in two separate runtimes)
    // TODO: if the two runtime issue is solved, maybe remove that...
    let send_future = send_msg_rx.for_each(move |(peer_pid, msg)| {
        shoal.lock().send_to(peer_pid, msg).map_err(|err| {
            eprintln!(
                "Executor : {} : Error while sending msg to peer `{}` : `{:?}`.",
                pode_id, peer_pid, err
            );
        })
    });
    runtime.spawn(send_future);

    tasks_tx
}

pub fn orchestrate(
    pode_id: PodeId,
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job>>>>>,
    job_instances: Arc<Mutex<Vec<(usize, wasm::ModuleRef)>>>,
    job_id: JobId,
    task_id: TaskId,
    send_msg_tx: Sender<(Pid, Message)>,
) -> Box<Future<Item = (), Error = Error>> {
    // println!("Time 1 : {:?}", Instant::now());
    let (bytecode, args, task) = {
        // TODO: deadlock/blocking job in case of multi-threading?
        // TODO: check job and task ids ?
        let jobs = jobs.lock().unwrap();
        let job = jobs.iter().find(|job| job.lock().unwrap().id == job_id);
        if let Some(job) = job {
            let job = job.lock().unwrap();
            let task = job
                .tasks
                .iter()
                .find(|task| task.lock().unwrap().id == task_id);
            if let Some(task) = task {
                let mut task_locked = task.lock().unwrap();
                task_locked.set_unavailable();
                //TODO: clone bytecode?
                (job.bytecode.clone(), task_locked.args.clone(), task.clone())
            } else {
                // TODO: proper error handling ?
                eprintln!("Pode : {} : The pode sent a return value correpsonding to an unknown task `{}` for job `{}`, discarding...", pode_id, task_id, job_id);
                return Box::new(future::err(Error::JobNotFound(job_id)));
            }
        } else {
            // TODO: proper error handling ?
            eprintln!("Pode : {} : The pode sent a return value corresponding to an unknown job `{}`, discarding...", pode_id, job_id);
            return Box::new(future::err(Error::TaskNotFound(job_id, task_id)));
        }
    };

    // println!("Time 2 : {:?}", Instant::now());

    // This little gymnastic is done to be able to cache job_instances.
    // TODO: There is probably a much cleaner way... :/
    let res = {
        // TODO: Locking for the whole execution ?
        let job_instances = job_instances.lock().unwrap();
        let instance_opt = match job_instances.iter().find(|(id, _)| id == &job_id) {
            Some((_, instance_opt)) => Some(instance_opt),
            None => None,
        };

        // println!("Time 3 : {:?}", Instant::now());
        // This is very slow when compiled in debug mode (much better in release)
        wasm::exec_wasm(&bytecode[..], &args, instance_opt)
    };

    // println!("Time 4 : {:?}", Instant::now());

    let res = match res {
        Ok((res, instance)) => {
            if let Some(instance) = instance {
                job_instances.lock().unwrap().push((job_id, instance));
            }
            Ok(res)
        }
        Err(err) => Err(err),
    };

    println!(
        "Pode : {} : Executed Task #{} for Job #{}",
        pode_id, task_id, job_id
    );

    //TODO: return proper error
    let res = res.map_err(|_| ());
    task.lock().unwrap().result = Some(res.clone());

    let send_future = send_msg_tx
        .send((MANAGER_ID, Message::ReturnValue(job_id, task_id, res)))
        .and_then(|sender| sender.send((MANAGER_ID, Message::Idle(NB_TASKS))))
        .map(|_| ())
        .map_err(|err| Error::ToShoalMpscError(err));

    // println!("Time 5 : {:?}", Instant::now());
    Box::new(send_future)
}
