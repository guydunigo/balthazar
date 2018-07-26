extern crate balthmessage as message;
//TODO: +everywhere stream or socket or ...

use std::convert::From;
use std::fmt::Display;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::thread::sleep;
use std::time::Duration;

use message::{Message, MessageReader};

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    FailedHandshake,
    IoError(io::Error),
    MessageError(message::Error),
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

// ------------------------------------------------------------------

pub fn swim<A: ToSocketAddrs + Display>(addr: A) -> Result<(), Error> {
    let mut socket = TcpStream::connect(&addr)?;
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

    Message::Connected(id).send(&mut socket)?;

    let mut reader = MessageReader::new(id, socket.try_clone()?);
    let result = {
        let mut socket = socket.try_clone()?;
        Message::Idle(1).send(&mut socket)?;
        reader.for_each_until_error(|msg| {
            if let Message::Job(_) = msg {
                println!("Received a job");
                Ok(())
            } else {
                sleep(Duration::from_secs(1));
                msg.send(&mut socket)
            }
        })
    };

    match result {
        Err(err) => Err(Error::from(err)),
        Ok(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
