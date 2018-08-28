#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;

pub use ron::{de, ser};
use std::io;
use std::io::prelude::*;
use std::iter::FusedIterator;

pub const BUFFER_SIZE: usize = 1024;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    SerError(ser::Error),
    DeError(de::Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
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

// ------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Hello(String),
    Connected(usize),
    // TODO: Useful to annouce deconnection ?
    Disconnect,
    Disconnected(usize),
    Idle(usize),
    Job(Vec<u8>),
    NoJob,
}

impl Message {
    pub fn send<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        let msg_str = ser::to_string(self)?;
        if let Message::Job(_) = self {
            println!("sending Job of {} bytes.", msg_str.len());
        } else {
            println!("sending `{}` of {} bytes.", msg_str, msg_str.len());
        }
        writer.write_all(msg_str.as_bytes())?;
        Ok(())
    }
}

pub struct MessageReader<R: Read> {
    id: usize,
    reader: Option<R>,
    buffer: Vec<u8>,
    n: u32,
}

impl<R: Read> MessageReader<R> {
    pub fn new(id: usize, reader: R) -> MessageReader<R> {
        MessageReader {
            id,
            reader: Some(reader),
            buffer: Vec::with_capacity(BUFFER_SIZE),
            n: 0,
        }
    }

    pub fn for_each_until_error<F>(&mut self, mut closure: F) -> Result<(), Error>
    where
        F: FnMut(Message) -> Result<(), Error>,
    {
        let id = self.id;
        let res =
            self.take_while(|result| match result {
                Ok(Message::Disconnect) => {
                    println!("{} : Disconnection announced.", id);
                    false
                }
                Ok(Message::Disconnected(_)) => {
                    println!("{} : Disconnected socket.", id);
                    false
                }
                _ => true,
            }).map(|msg_res| -> Result<(), Error> {
                    match msg_res {
                        Ok(msg) => closure(msg),
                        Err(err) => Err(err),
                    }
                })
                .skip_while(|result| result.is_ok())
                .next();

        match res {
            Some(Err(err)) => Err(err),
            _ => Ok(()),
        }
    }
}

impl<R: Read> FusedIterator for MessageReader<R> {}

impl<R: Read> Iterator for MessageReader<R> {
    type Item = Result<Message, Error>;

    // TODO: clean this mess... (multiple returns ...)
    fn next(&mut self) -> Option<Result<Message, Error>> {
        if let Some(mut reader) = self.reader.take() {
            let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
            loop {
                let n = match reader.read(&mut buffer) {
                    Ok(n) => n,
                    Err(err) => return Some(Err(Error::from(err))),
                };
                if n <= 0 {
                    // TODO: Or directly return none...
                    return Some(Ok(Message::Disconnected(self.id)));
                }

                /*let capacity = self.buffer.capacity();
                if capacity - self.buffer.len() < BUFFER_SIZE {
                    self.buffer.reserve(capacity);
                }*/
                self.buffer.extend_from_slice(&buffer[..n]);
                self.n += 1;

                println!("{} {} {} {}", self.n, n, self.buffer.len(), self.buffer.capacity());

                let msg_res: de::Result<Message> = de::from_bytes(&mut self.buffer.as_slice());
                let res = match msg_res {
                    Ok(msg) => {
                        if let Message::Job(_) = msg {
                            println!("{} : received a Job.", self.id);
                        } else {
                            println!("{} : received `{:?}`.", self.id, msg);
                        }
                        self.reader = Some(reader);
                        Ok(msg)
                    }
                    Err(de::Error::Message(msg)) => {
                        println!("{} : invalid message `{}`", self.id, msg);
                        continue;
                    }
                    // TODO: useful anymore ?
                    /*Err(de::Error::Parser(de::ParseError::Eof, a)) => {
                        println!("{:?}",a);
                        Ok(Message::Disconnected(self.id))
                    }*/
                    Err(de::Error::Parser(err, _)) => {
                        println!("{} : parse error `{:?}`", self.id, err);
                        continue;
                    }
                    Err(de::Error::IoError(err)) => {
                        println!("{} : IoError `{}`", self.id, err);
                        Err(Error::from(de::Error::IoError(err)))
                    }
                };

                self.buffer.clear();
                // TODO? : self.buffer.shrink_to(BUFFER_SIZE);
                return Some(res);
            }
        } else {
            return None;
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
