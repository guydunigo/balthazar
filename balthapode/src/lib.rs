extern crate balthajob as job;
extern crate balthmessage as message;
extern crate parity_wasm;
extern crate wasmi;

mod orchestrator;

//TODO: +everywhere stream or socket or ...

use std::convert::From;
use std::fmt::Display;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::Mutex;

use job::task::arguments::Arguments;
use job::task::{LoneTask, Task};
use job::Job;
use message::{de, Message, MessageReader};

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    FailedHandshake,
    IoError(io::Error),
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

pub fn initialize_pode<A: ToSocketAddrs + Display>(addr: A) -> Result<(TcpStream, usize), Error> {
    let socket = TcpStream::connect(&addr)?;
    println!("Connected to : `{}`", addr);

    //TODO: as option
    let id = {
        let mut init_reader = MessageReader::new(0, socket.try_clone()?);
        match init_reader.next() {
            Some(Ok(Message::Connected(id))) => Ok(id),
            _ => Err(Error::FailedHandshake),
        }
    }?;
    println!("Handshake successful, received id : {}.", id);

    Ok((socket, id))
}

pub fn fill<A: ToSocketAddrs + Display>(addr: A) -> Result<(), Error> {
    let (mut socket, id) = initialize_pode(addr)?;

    {
        let mut f = File::open("main.wasm")?;
        let mut code: Vec<u8> = Vec::new();
        f.read_to_end(&mut code)?;

        Message::Job(0, code).send(&mut socket)?;
    }
    let job_id = {
        let mut reader = MessageReader::new(id, socket.try_clone()?);
        match reader.next() {
            Some(Ok(Message::JobRegisteredAt(job_id))) => job_id,
            Some(Ok(msg)) => return Err(Error::UnexpectedReply(msg)),
            Some(Err(err)) => return Err(Error::from(err)),
            None => return Err(Error::NoReply),
        }
    };
    {
        let mut f = File::open("args_list.ron")?;
        let mut args_list_txt: Vec<u8> = Vec::new();
        f.read_to_end(&mut args_list_txt)?;

        // Deserialize args_list
        let mut args_list: Vec<Arguments> = de::from_bytes(&args_list_txt[..])?;

        for args in args_list.drain(..) {
            Message::Task(job_id, 0, args).send(&mut socket)?;
        }
    }

    Ok(())
}

pub fn swim<A: ToSocketAddrs + Display>(addr: A) -> Result<(), Error> {
    let (socket, id) = initialize_pode(addr)?;

    let mut reader = MessageReader::new(id, socket.try_clone()?);
    let result = {
        let mut socket = Arc::new(Mutex::new(socket));
        let mut lone_tasks: Vec<LoneTask<bool>> = Vec::new();

        let jobs: Vec<Arc<Mutex<Job<bool>>>> = Vec::new();
        let jobs = Arc::new(Mutex::new(jobs));

        orchestrator::start_orchestrator(jobs.clone(), socket.clone());

        reader.for_each_until_error(|msg| match msg {
            Message::Job(job_id, bytecode) => {
                let (new_lone_tasks, res) =
                    register_job(jobs.clone(), &mut lone_tasks, job_id, bytecode);
                lone_tasks = new_lone_tasks;
                res
            }
            Message::Task(job_id, task_id, args) => register_task(
                jobs.clone(),
                &mut lone_tasks,
                socket.clone(),
                job_id,
                task_id,
                args,
            ),
            _ => {
                /*{
                    let mut socket = socket.lock().unwrap();
                    Message::Disconnect.send(&mut *socket)
                }*/
                Ok(())
            }
        })
    };

    match result {
        Err(err) => Err(Error::from(err)),
        Ok(_) => Ok(()),
    }
}

fn register_job(
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job<bool>>>>>>,
    lone_tasks: &mut Vec<LoneTask<bool>>,
    job_id: usize,
    bytecode: Vec<u8>,
) -> (Vec<LoneTask<bool>>, Result<(), message::Error>) {
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

    (new_lone_tasks, Ok(()))
}

fn register_task(
    jobs: Arc<Mutex<Vec<Arc<Mutex<Job<bool>>>>>>,
    lone_tasks: &mut Vec<LoneTask<bool>>,
    socket: Arc<Mutex<TcpStream>>,
    job_id: usize,
    task_id: usize,
    args: Arguments,
) -> Result<(), message::Error> {
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
        Some(job) => job.lock().unwrap().push_new_task(task_id, args),
        None => {
            let task = Task::new(task_id, args);
            lone_tasks.push(LoneTask { job_id, task });
            {
                let mut socket = socket.lock().unwrap();
                Message::RequestJob(job_id).send(&mut *socket)?;
            }
        }
    }

    println!("Task #{} for Job #{} saved.", task_id, job_id);

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
