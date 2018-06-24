mod manager;

use std::io;
use std::sync::mpsc::{Receiver, RecvError};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::io::prelude::*;

#[derive(Debug)]
pub enum Error {
    ListenerRecvError(RecvError), // Should only happen when the other end is disconnected
    IoError(io::Error),
}

impl From<RecvError> for Error {
    fn from(err: RecvError) -> Error {
        Error::ListenerRecvError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

pub fn orchestrate(rx: Receiver<TcpStream>) -> Result<(), Error> {
    let mut stream = rx.recv()?;

    let peer_addr = stream.peer_addr()?;
    println!("New peer at address : `{}`", peer_addr);

    thread::sleep(Duration::from_secs(5));
    let buffer = b"test";
    stream.write_all(buffer)?;
    stream.flush()?;

    Ok(())
}
