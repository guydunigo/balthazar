use std::convert::From;
use std::fmt::Display;
use std::io;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{SendError, Sender};

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    OrchestratorSendError(SendError<TcpStream>),
    IoError(io::Error),
}

impl From<SendError<TcpStream>> for Error {
    fn from(err: SendError<TcpStream>) -> Error {
        Error::OrchestratorSendError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

// ------------------------------------------------------------------

// TODO: channel to communicate orders like shutdown ?
pub fn listen<A: ToSocketAddrs + Display>(
    listen_addr: A,
    tx: &Sender<TcpStream>,
) -> Result<(), Error> {
    // TODO: Look at different errors
    let listener = TcpListener::bind(&listen_addr)?;
    println!("Listening on : `{}`", listen_addr);

    /*for stream in self.0.incoming() {
        // TODO: Look at different errors
        let mut stream = stream?;
        let peer_addr = stream.peer_addr()?;
        println!("New peer at address : `{}`", peer_addr);
    
        self.pods.push(stream);
        println!("Size of the pods list : {}", self.pods.len());
    }*/

    listener
        .incoming()
        .map(|stream| -> Result<(), Error> {
            let stream = stream?;
            Ok(tx.send(stream)?)
        })
        .skip_while(|result| result.is_ok())
        .next()
        .unwrap()?;
    //.for_each(|res| eprintln!("{:?}", res));

    Ok(())
}
