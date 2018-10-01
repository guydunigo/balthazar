use std::io;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use job;
use job::wasm;
use job::Job;
use message;
use message::Message;

const SLEEP_TIME_MS: u64 = 1000;
const NB_TASKS: usize = 1;

// ------------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    WasmError(wasm::Error),
    MessageError(message::Error),
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

// ------------------------------------------------------------------

pub fn start_orchestrator(
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job<bool>>>>>>,
    cephalo: Arc<Mutex<TcpStream>>,
) -> Receiver<bool> {
    let (tx, rx) = mpsc::sync_channel(0);
    thread::spawn(move || orchestrate(jobs, cephalo, tx));
    rx
}

pub fn orchestrate(
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job<bool>>>>>>,
    cephalo: Arc<Mutex<TcpStream>>,
    tx: SyncSender<bool>,
) -> Result<(), Error> {
    let mut last_was_nojob = false;

    loop {
        let task_opt = {
            let jobs = jobs.lock().unwrap();
            job::get_available_task(&*jobs)
        };
        if let Some((job, task)) = task_opt {
            last_was_nojob = false;

            let (task_id, args) = {
                let task = task.lock().unwrap();
                (task.id, task.args.clone())
            };
            //TODO: clone bytecode?
            let (job_id, bytecode) = {
                let job = job.lock().unwrap();
                (job.id, job.bytecode.clone())
            };

            let res = wasm::exec_wasm(&bytecode[..], &args);
            println!(
                "Executed Task #{} for Job #{} : `{:?}`",
                task_id, job_id, res
            );

            //TODO: return proper error
            let res = match res {
                Ok(args) => Ok(args),
                Err(_) => Err(()),
            };
            task.lock().unwrap().result = Some(res.clone());

            {
                let mut cephalo = cephalo.lock().unwrap();
                Message::ReturnValue(job_id, task_id, res).send(&mut *cephalo)?;
            }
        } else {
            {
                let mut cephalo = cephalo.lock().unwrap();
                Message::Idle(NB_TASKS).send(&mut *cephalo)?;
            }

            tx.send(true).unwrap();

            if last_was_nojob {
                println!("Orchestrator sleeping...");
                thread::sleep(Duration::from_millis(SLEEP_TIME_MS));
            }
            last_was_nojob = true;
        }
    }
}
