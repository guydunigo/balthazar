use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const BUF_SIZE: usize = 1024;

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    OrchestratorTxError(mpsc::SendError<Message>),
    AlreadyManagedError,
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

pub struct Manager {
    id: usize,
    stream: Option<TcpStream>,
    orch_tx: Option<mpsc::Sender<Message>>,
    handle: Option<thread::JoinHandle<Result<(), Error>>>,
    _job: Option<Arc<()>>,
}

impl Manager {
    pub fn new(id: usize, stream: TcpStream, orch_tx: mpsc::Sender<Message>) -> Manager {
        Manager {
            id,
            stream: Some(stream),
            orch_tx: Some(orch_tx),
            handle: None,
            _job: None,
        }
    }

    pub fn manage(&mut self) -> Result<(), Error> {
        if let (None, Some(stream), Some(orch_tx)) =
            (&self.handle, self.stream.take(), self.orch_tx.take())
        {
            let id = self.id;
            self.handle = Some(thread::spawn(move || manage(id, stream, orch_tx)));
        } else {
            return Err(Error::AlreadyManagedError);
        }

        Ok(())
    }
}

pub fn manage(
    id: usize,
    mut stream: TcpStream,
    orch_tx: mpsc::Sender<Message>,
) -> Result<(), Error> {
    let peer_addr = stream.peer_addr()?;
    println!("New Pode {} at address : `{}`", id, peer_addr);

    let buffer = b"test";
    stream.write_all(buffer)?;
    stream.flush()?;

    let mut buffer = [0; BUF_SIZE];
    let mut n = stream.read(&mut buffer)?;
    while n > 0 {
        println!(
            "Pode {} : received `{}`.",
            id,
            String::from_utf8_lossy(&buffer[..n])
        );
        n = stream.read(&mut buffer)?;
    }
    // thread::sleep(Duration::from_secs(5));

    println!("Pode {} : Disconnected, notifying orchestrator...", id);
    orch_tx.send(Message::Disconnected(id))?;

    Ok(())
}

impl Drop for Manager {
    fn drop(&mut self) {
        println!("Pode {} : Dropping...", self.id);

        if let Some(handle) = self.handle.take() {
            println!("Pode {} : Joining the thread...", self.id);
            handle.join().unwrap().unwrap();
        } else {
            println!("Pode {} : Closing the stream...", self.id);
            self.stream
                .take()
                .unwrap()
                .shutdown(Shutdown::Both)
                .unwrap();
        }

        println!("Pode {} : Deleted", self.id);
    }
}

pub enum Message {
    Disconnected(usize),
}
