#![feature(int_to_from_bytes)]
#![feature(vec_resize_default)]

#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;

pub use ron::{de, ser};
use std::io;
use std::io::prelude::*;
use std::iter::FusedIterator;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    SerError(ser::Error),
    DeError(de::Error),
    CouldNotGetSize,
    MessageTooBig(usize),
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
    Job(usize, Vec<u8>),                     // TODO: Job ids?
    ReturnValue(usize, Result<Vec<u8>, ()>), // TODO: proper error
    // External(E) // TODO: generic type
    NoJob,
}

impl Message {
    pub fn send<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        let msg_str = ser::to_string(self)?;
        let len = msg_str.len();

        if let Message::Job(id, _) = self {
            println!("sending Job #{} of {} bytes.", id, len);
        } else {
            println!("sending `{}` of {} bytes.", msg_str, len);
        }

        if len >= u32::max_value() as usize {
            return Err(Error::MessageTooBig(len));
        }
        writer.write_all(&(len as u32).to_le_bytes())?;

        writer.write_all(msg_str.as_bytes())?;
        Ok(())
    }
}

pub struct MessageReader<R: Read> {
    id: usize,
    reader: Option<R>,
}

impl<R: Read> MessageReader<R> {
    pub fn new(id: usize, reader: R) -> MessageReader<R> {
        MessageReader {
            id,
            reader: Some(reader),
        }
    }

    pub fn for_each_until_error<F>(&mut self, mut closure: F) -> Result<(), Error>
    where
        F: FnMut(Message) -> Result<(), Error>,
    {
        let id = self.id;
        let res = self
            .take_while(|result| match result {
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
            }).skip_while(|result| result.is_ok())
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
            let msg_size: usize = {
                let mut buffer: [u8; 4] = [0; 4];

                let n = match reader.read(&mut buffer) {
                    Ok(n) => n,
                    Err(err) => return Some(Err(Error::from(err))),
                };
                if n != 4 {
                    return Some(Err(Error::CouldNotGetSize));
                }

                u32::from_le_bytes(buffer) as usize
            };
            println!("{} : Receiving {} bytes...", self.id, msg_size);

            let mut buffer: Vec<u8> = Vec::new();
            buffer.resize_default(msg_size);

            // Loops until it got the full message:
            let mut downloaded_size = 0;
            while downloaded_size < buffer.len() {
                let n = match reader.read(&mut buffer[downloaded_size..]) {
                    Ok(n) => n,
                    Err(err) => return Some(Err(Error::from(err))),
                };
                if n <= 0 {
                    // TODO: Or directly return none...
                    return Some(Ok(Message::Disconnected(self.id)));
                }

                downloaded_size += n;
            }

            let msg_res: de::Result<Message> = de::from_bytes(&mut buffer.as_slice());
            let res = match msg_res {
                Ok(msg) => {
                    if let Message::Job(job_id, _) = msg {
                        println!("{} : received Job #{}.", self.id, job_id);
                    } else {
                        println!("{} : received `{:?}`.", self.id, msg);
                    }
                    self.reader = Some(reader);
                    Ok(msg)
                }
                Err(err) => Err(Error::from(err)),
            };
            return Some(res);
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
