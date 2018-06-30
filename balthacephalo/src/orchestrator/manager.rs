use std::io;
use std::io::prelude::*;
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use message::{de, ser, Message};

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    OrchestratorTxError(mpsc::SendError<Message>),
    SerError(ser::Error),
    DeError(de::Error),
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

impl From<ser::Error> for Error {
    fn from(err: ser::Error) -> Error {
        Error::SerError(err)
    }
}

impl From<de::Error> for Error {
    fn from(err: de::Error) -> Error {
        Error::DeError(err)
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

    let msg = Message::Hello("salut".to_string());
    let msg_str = ser::to_string(&msg)?;

    stream.write_all(msg_str.as_bytes())?;
    // stream.flush()?;

    // let mut buffer = [0; BUF_SIZE];
    // let mut n = stream.read(&mut buffer)?;
    loop {
        let msg_res: de::Result<Message> = de::from_reader(&mut stream);
        match msg_res {
            Ok(msg) => println!("Manager {} : received `{:?}`.", id, msg),
            Err(de::Error::Message(msg)) => println!("Manager {} : invalid message '{}'", id, msg),
            Err(de::Error::Parser(de::ParseError::Eof, _)) => {
                // println!("Manager {} : EOF", id);
                break;
            }
            Err(de::Error::Parser(err, _)) => println!("Manager {} : parse error `{:?}`", id, err),
            Err(de::Error::IoError(err)) => {
                println!("Manager {} : IoError `{}`", id, err);
                break;
            }
        };
    }

    // println!("Manager {} : Disconnected, notifying orchestrator...", id);
    // TODO: Report errors ?
    orch_tx.send(Message::Disconnected(id))?;

    Ok(())
}

impl Drop for Manager {
    fn drop(&mut self) {
        // println!("Manager {} : Dropping...", self.id);

        if let Some(handle) = self.handle.take() {
            // println!("Manager {} : Joining the thread...", self.id);
            handle.join().unwrap().unwrap();
        } else {
            // println!("Manager {} : Closing the stream...", self.id);
            self.stream
                .take()
                .unwrap()
                .shutdown(Shutdown::Both)
                .unwrap();
        }

        // println!("Manager {} : Deleted", self.id);
    }
}
