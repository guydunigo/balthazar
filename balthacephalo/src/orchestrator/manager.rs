use std::io;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

// TODO: replace TcpStream by Read + Write

use job;
use job::task::Task;
use job::Job;
use message;
use message::{Message, MessageReader};

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    OrchestratorTxError(mpsc::SendError<Message>),
    MessageError(message::Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<mpsc::SendError<Message>> for Error {
    fn from(err: mpsc::SendError<Message>) -> Error {
        Error::OrchestratorTxError(err)
    }
}

impl From<message::Error> for Error {
    fn from(err: message::Error) -> Error {
        Error::MessageError(err)
    }
}

// ------------------------------------------------------------------

pub struct Manager {
    id: usize,
    handle: Option<thread::JoinHandle<Result<(), Error>>>,
    // TODO: Job as well ?
    task: Option<Arc<Mutex<Task<Mutex<Manager>>>>>,
}

impl Manager {
    pub fn new(
        id: usize,
        stream: TcpStream,
        orch_tx: mpsc::Sender<Message>,
        jobs_rc: Arc<Mutex<Vec<Arc<Job<Mutex<Manager>>>>>>,
    ) -> Arc<Mutex<Manager>> {
        let man = Arc::from(Mutex::new(Manager {
            id,
            handle: None, // TODO: useful ?
            task: None,
        }));

        let clone = man.clone();

        {
            man.lock().unwrap().handle = Some(thread::spawn(move || {
                let res = Manager::manage(clone, stream, orch_tx, jobs_rc);
                println!("{} : {:?}", id, res);
                res
            }));
        }

        man
    }

    pub fn manage(
        manager: Arc<Mutex<Manager>>,
        mut stream: TcpStream,
        orch_tx: mpsc::Sender<Message>,
        jobs_rc: Arc<Mutex<Vec<Arc<Job<Mutex<Manager>>>>>>,
    ) -> Result<(), Error> {
        let man_id = manager.lock().unwrap().id;
        let peer_addr = stream.peer_addr()?;
        println!("New Pode {} at address : `{}`", man_id, peer_addr);

        Message::Connected(man_id).send(&mut stream)?;

        let mut reader = MessageReader::new(man_id, stream.try_clone()?);
        let result = {
            let mut stream = stream.try_clone()?;
            reader.for_each_until_error(|msg| match msg {
                Message::Idle(i) => {
                    // TODO: check if the pode doesn't already have a task:
                    //      - Free the previous task
                    //      - Send again the previous task
                    //      - Send error
                    for _ in 0..i {
                        let mut jobs = jobs_rc.lock().unwrap();
                        match job::get_available_task(&*jobs) {
                            Some((job, task)) => {
                                let send_res = {
                                    let mut task = task.lock().unwrap();
                                    // If the sending fails, we don't register the task.
                                    let send_res =
                                        Message::Job(job.id, task.id, job.bytecode.clone())
                                            .send(&mut stream)?;
                                    task.pode = Some(Arc::downgrade(&manager));
                                    send_res
                                };
                                {
                                    let mut manager = manager.lock().unwrap();
                                    manager.task = Some(task.clone());
                                }
                                send_res
                            }
                            None => {
                                Message::NoJob.send(&mut stream)?;
                                break;
                            }
                        }
                    }
                    Ok(())
                }
                Message::Job(_, _, job) => {
                    let mut jobs = jobs_rc.lock().unwrap();
                    let mut job =
                        Job::new(Job::get_free_job_id(&jobs[..]).unwrap(), job, Vec::new());
                    job.new_task(Vec::new());

                    jobs.push(Arc::new(job));
                    Ok(())
                }
                Message::ReturnValue(_, _, value) => {
                    // TODO: deadlock?
                    // TODO: check job and task ids ?
                    let mut manager = manager.lock().unwrap();
                    let task = manager.task.clone();
                    if let Some(task) = task {
                        {
                            let mut task = task.lock().unwrap();
                            task.result = Some(value);
                            task.pode = None;
                        }
                        manager.task = None;
                    } else {
                        // TODO: proper error handling ?
                        println!("{} : The pode sent a return value but doesn't have a linked task, discarding...", man_id);
                    }
                    Ok(())
                }
                _ => Message::Hello("Hey".to_string()).send(&mut stream),
            })
        };

        // println!("Manager {} : Disconnected, notifying orchestrator...", man_id);
        // TODO: Report errors ?
        // TODO: useful if already EOF ?
        Message::Disconnect.send(&mut stream).unwrap_or_default();
        orch_tx.send(Message::Disconnected(man_id))?;

        match result {
            Err(err) => Err(Error::from(err)),
            Ok(_) => Ok(()),
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
