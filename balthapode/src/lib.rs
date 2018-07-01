extern crate balthmessage as message;
//TODO: +everywhere stream or socket or ...

use std::convert::From;
use std::fmt::Display;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::thread::sleep;
use std::time::Duration;

use message::{Message, MessageReader};

#[derive(Debug)]
pub enum Error {
    AlreadyUsed,
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

pub struct Pode {
    //TODO: as option
    id: usize,
    cephalo: Option<TcpStream>,
}

impl Pode {
    pub fn new<A: ToSocketAddrs + Display>(addr: A) -> Result<Pode, Error> {
        let socket = TcpStream::connect(&addr)?;
        println!("Connected to : `{}`", addr);

        Ok(Pode {
            id: 0,
            cephalo: Some(socket),
        })
    }

    pub fn swim(&mut self) -> Result<(), Error> {
        if let Some(mut socket) = self.cephalo.take() {
            self.id = {
                let mut init_reader = MessageReader::new(self.id, socket.try_clone()?);
                match init_reader.next() {
                    Some(Ok(Message::Connected(id))) => Ok(id),
                    _ => Err(Error::FailedHandshake),
                }
            }?;
            println!("Handshake successful, received id : {}.", self.id);

            Message::Connected(self.id).send(&mut socket)?;

            let mut reader = MessageReader::new(self.id, socket.try_clone()?);
            let result = {
                let mut socket = socket.try_clone()?;
                reader.for_each_until_error(|msg| {
                    sleep(Duration::from_secs(1));
                    msg.send(&mut socket)
                })
            };

            self.cephalo = Some(socket);

            match result {
                Err(err) => Err(Error::from(err)),
                Ok(_) => Ok(()),
            }
        } else {
            Err(Error::AlreadyUsed)
        }
    }
}

impl Drop for Pode {
    fn drop(&mut self) {
        if let Some(mut socket) = self.cephalo.take() {
            Message::Disconnect.send(&mut socket).unwrap_or_default();
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
