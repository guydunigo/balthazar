extern crate balthmessage as message;
//TODO: +everywhere stream or socket or ...

use std::convert::From;
use std::fmt::Display;
use std::io;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::thread::sleep;
use std::time::Duration;

use message::{Message, MessageReader};

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    FailedHandshake,
    ReadError(message::ReadError),
    WriteError(message::WriteError),
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
    cephalo: Option<TcpStream>,
}

impl Pode {
    pub fn new<A: ToSocketAddrs + Display>(addr: A) -> Result<Pode, Error> {
        let socket = TcpStream::connect(&addr)?;
        println!("Connected to : `{}`", addr);

        Ok(Pode {
            cephalo: Some(socket),
        })
    }

    pub fn swim(&mut self) -> Result<(), Error> {
        if let Some(mut socket) = self.cephalo.take() {
            let id = {
                let mut init_reader = MessageReader::new(0, socket.try_clone()?);
                match init_reader.next() {
                    Some(Ok(Message::Connected(id))) => Ok(id),
                    _ => Err(Error::FailedHandshake),
                }
            }?;
            println!("Handshake successful, received id : {}.", id);

            Message::Connected(id).send(&mut socket)?;

            let reader = MessageReader::new(id, socket.try_clone()?);
            reader
                .map(|msg_res| -> Result<(), Error> {
                    sleep(Duration::from_secs(1));
                    match msg_res {
                        Ok(msg) => {
                            println!("Received : `{:?}`", msg);
                            msg.send(&mut socket)?;
                        }
                        Err(err) => return Err(Error::from(err)),
                    }

                    Ok(())
                })
                .skip_while(|result| result.is_ok())
                .next()
                .unwrap()?;
        }

        Ok(())
    }
}

impl Drop for Pode {
    fn drop(&mut self) {
        println!("Closing socket...");
        // TODO: Send stop signal before closing
        if let Some(socket) = self.cephalo.take() {
            socket.shutdown(Shutdown::Both).unwrap();
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
