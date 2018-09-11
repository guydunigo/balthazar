use std::io;
use std::net::TcpStream;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

// TODO: replace TcpStream by Read + Write

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
    _job: Option<Arc<()>>,
}

impl Manager {
    pub fn new(
        id: usize,
        stream: TcpStream,
        orch_tx: mpsc::Sender<Message>,
        jobs_rc: Arc<Mutex<Vec<Vec<u8>>>>,
    ) -> Manager {
        let handle = Some(thread::spawn(move || manage(id, stream, orch_tx, jobs_rc)));

        Manager {
            id,
            handle,
            _job: None,
        }
    }
}

pub fn manage(
    id: usize,
    mut stream: TcpStream,
    orch_tx: mpsc::Sender<Message>,
    jobs_rc: Arc<Mutex<Vec<Vec<u8>>>>,
) -> Result<(), Error> {
    let peer_addr = stream.peer_addr()?;
    println!("New Pode {} at address : `{}`", id, peer_addr);

    Message::Connected(id).send(&mut stream)?;

    let mut reader = MessageReader::new(id, stream.try_clone()?);
    let result = {
        let mut stream = stream.try_clone()?;
        reader.for_each_until_error(|msg| match msg {
            Message::Idle(i) => {
                for _ in 0..i {
                    match jobs_rc.lock().unwrap().pop() {
                        Some(job) => Message::Job(job).send(&mut stream)?,
                        None => {
                            Message::NoJob.send(&mut stream)?;
                            break;
                        }
                    }
                }
                Ok(())
            }
            Message::Job(job) => {
                jobs_rc.lock().unwrap().push(job);
                Ok(())
            }
            _ => Message::Hello("Hey".to_string()).send(&mut stream),
        })
    };

    // println!("Manager {} : Disconnected, notifying orchestrator...", id);
    // TODO: Report errors ?
    Message::Disconnect.send(&mut stream).unwrap_or_default();
    orch_tx.send(Message::Disconnected(id))?;

    match result {
        Err(err) => Err(Error::from(err)),
        Ok(_) => Ok(()),
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let res = handle.join();
            match res {
                Err(err) => println!(
                    "{} : Couldn't join the thread (it might have panicked) : {:?}",
                    self.id, err
                ),
                Ok(Err(err)) => println!(
                    "{} : The manager returned the following errors : {:?}",
                    self.id, err
                ),
                Ok(Ok(_)) => (), // println!("{} : The manager closed properly.", self.id),
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
