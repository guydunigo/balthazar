#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;

pub use ron::{de, ser};
use std::io;
use std::io::prelude::*;
use std::iter::FusedIterator;

pub const BUFFER_SIZE: usize = 1024;

#[derive(Debug)]
pub enum WriteError {
    IoError(io::Error),
    SerError(ser::Error),
}

impl From<io::Error> for WriteError {
    fn from(err: io::Error) -> WriteError {
        WriteError::IoError(err)
    }
}

impl From<ser::Error> for WriteError {
    fn from(err: ser::Error) -> WriteError {
        WriteError::SerError(err)
    }
}

#[derive(Debug)]
pub enum ReadError {
    IoError(io::Error),
    DeError(de::Error),
}

impl From<io::Error> for ReadError {
    fn from(err: io::Error) -> ReadError {
        ReadError::IoError(err)
    }
}

impl From<de::Error> for ReadError {
    fn from(err: de::Error) -> ReadError {
        ReadError::DeError(err)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Hello(String),
    Connected(usize),
    Disconnect,
    Disconnected(usize),
    Idle(usize),
}

impl Message {
    pub fn send<W: Write>(&self, writer: &mut W) -> Result<(), WriteError> {
        let msg_str = ser::to_string(self)?;
        writer.write_all(msg_str.as_bytes())?;
        Ok(())
    }
}

pub struct MessageReader<R: Read> {
    id: usize,
    reader: Option<R>,
    buffer: Vec<u8>,
}

impl<R: Read> MessageReader<R> {
    pub fn new(id: usize, reader: R) -> MessageReader<R> {
        MessageReader {
            id,
            reader: Some(reader),
            buffer: Vec::with_capacity(BUFFER_SIZE),
        }
    }
}

impl<R: Read> FusedIterator for MessageReader<R> {}

impl<R: Read> Iterator for MessageReader<R> {
    type Item = Result<Message, ReadError>;

    // TODO: clean this mess... (multiple returns ...)
    fn next(&mut self) -> Option<Result<Message, ReadError>> {
        if let Some(mut reader) = self.reader.take() {
            loop {
                let mut buffer: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
                let n = match reader.read(&mut buffer) {
                    Ok(n) => n,
                    Err(err) => return Some(Err(ReadError::IoError(err))),
                };
                if n <= 0 {
                    return Some(Ok(Message::Disconnected(self.id)));
                }
                buffer[..n].iter().for_each(|b| self.buffer.push(b.clone()));

                let msg_res: de::Result<Message> = de::from_bytes(&mut self.buffer.as_slice());
                let res = match msg_res {
                    Ok(msg) => {
                        println!("Manager {} : received `{:?}`.", self.id, msg);
                        self.reader = Some(reader);
                        Ok(msg)
                    }
                    Err(de::Error::Message(msg)) => {
                        println!("Manager {} : invalid message '{}'", self.id, msg);
                        continue;
                    }
                    // TODO: useful anymore ?
                    Err(de::Error::Parser(de::ParseError::Eof, _)) => {
                        Ok(Message::Disconnected(self.id))
                    }
                    Err(de::Error::Parser(err, _)) => {
                        println!("Manager {} : parse error `{:?}`", self.id, err);
                        continue;
                    }
                    Err(de::Error::IoError(err)) => {
                        println!("Manager {} : IoError `{}`", self.id, err);
                        Err(ReadError::from(de::Error::IoError(err)))
                    }
                };

                self.buffer.clear();
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
