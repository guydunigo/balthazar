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
    IoError(io::Error),
    FailedHandshake,
    ReadError(message::ReadError),
    WriteError(message::WriteError),
    AlreadyUsed,
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl From<message::ReadError> for Error {
    fn from(err: message::ReadError) -> Error {
        Error::ReadError(err)
    }
}

impl From<message::WriteError> for Error {
    fn from(err: message::WriteError) -> Error {
        Error::WriteError(err)
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

            let reader = MessageReader::new(self.id, socket.try_clone()?);
            let result = reader
                .map(|msg_res| -> Result<Message, Error> {
                    match msg_res {
                        Ok(res) => Ok(res),
                        Err(err) => Err(Error::from(err)),
                    }
                })
                .take_while(|result| match result {
                    Ok(Message::Disconnect) => {
                        println!("{} : Disconnection announced.", self.id);
                        false
                    }
                    Ok(Message::Disconnected(_)) => {
                        println!("{} : Disconnected socket.", self.id);
                        false
                    }
                    _ => true,
                })
                .map(|msg_res| -> Result<Message, Error> {
                    sleep(Duration::from_secs(1));

                    if let Ok(msg) = &msg_res {
                        msg.send(&mut socket)?;
                    }

                    msg_res
                })
                .skip_while(|result| result.is_ok())
                .next();

            self.cephalo = Some(socket);

            return match result {
                Some(Err(err)) => Err(err),
                _ => Ok(()),
            };
        }

        Err(Error::AlreadyUsed)
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
