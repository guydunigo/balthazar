use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use job;
use job::wasm;
use job::Job;
use message::Message;

const SLEEP_TIME_MS: u64 = 1000;

// ------------------------------------------------------------------

#[derive(Debug)]
pub enum Error {
    SendError(mpsc::SendError<Message>),
    WasmError(wasm::Error),
}

impl From<mpsc::SendError<Message>> for Error {
    fn from(err: mpsc::SendError<Message>) -> Error {
        Error::SendError(err)
    }
}

impl From<wasm::Error> for Error {
    fn from(err: wasm::Error) -> Error {
        Error::WasmError(err)
    }
}

// ------------------------------------------------------------------

pub fn start_orchestrator(jobs: Arc<Mutex<Vec<Arc<Mutex<Job<bool>>>>>>) -> mpsc::Receiver<Message> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || orchestrate(jobs, tx));

    rx
}

pub fn orchestrate(
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job<bool>>>>>>,
    sender: mpsc::Sender<Message>,
) -> Result<(), Error> {
    loop {
        let task_opt = {
            let jobs = jobs.lock().unwrap();
            job::get_available_task(&*jobs)
        };
        if let Some((job, task)) = task_opt {
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

            sender.send(Message::ReturnValue(job_id, task_id, res))?;
        } else {
            // TODO: How to wait for new jobs?
            println!("Sleeping...");
            thread::sleep(Duration::from_millis(SLEEP_TIME_MS));
        }
    }
}
