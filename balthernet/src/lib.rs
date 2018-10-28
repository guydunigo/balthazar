extern crate balthmessage as message;

use std::fmt::Display;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};

use balthmessage::{Message, MessageReader};

pub mod listener;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    FailedHandshake,
    MessageError(message::Error),
    IoError(io::Error),
    ListenerError(listener::Error),
}

impl From<message::Error> for Error {
    fn from(err: message::Error) -> Error {
        Error::MessageError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<listener::Error> for Error {
    fn from(err: listener::Error) -> Error {
        Error::ListenerError(err)
    }
}

// ------------------------------------------------------------------

pub fn initialize_pode<A: ToSocketAddrs + Display>(addr: A) -> Result<(TcpStream, usize), Error> {
    let socket = TcpStream::connect(&addr)?;
    println!("Connected to : `{}`", addr);

    //TODO: as option
    let pode_id = {
        let mut init_reader = MessageReader::new(0, socket.try_clone()?);
        match init_reader.next() {
            Some(Ok(Message::Connected(pode_id))) => Ok(pode_id),
            _ => Err(Error::FailedHandshake),
        }
    }?;
    println!("{} : Handshake successful.", pode_id);

    Ok((socket, pode_id))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
