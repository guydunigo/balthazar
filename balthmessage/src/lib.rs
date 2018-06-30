#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;

pub use ron::{de, ser};
use std::io::prelude::*;
use std::iter::FusedIterator;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Hello(String),
    Connected(usize),
    Disconnect,
    Disconnected(usize),
    Idle(usize),
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
}

impl<R: Read> FusedIterator for MessageReader<R> {}

impl<R: Read> Iterator for MessageReader<R> {
    type Item = de::Result<Message>;

    // TODO: clean this mess...
    fn next(&mut self) -> Option<de::Result<Message>> {
        if let Some(mut reader) = self.reader.take() {
            loop {
                let msg_res: de::Result<Message> = de::from_reader(&mut reader);
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
                    Err(de::Error::Parser(de::ParseError::Eof, _)) => {
                        Ok(Message::Disconnected(self.id))
                    }
                    Err(de::Error::Parser(err, _)) => {
                        println!("Manager {} : parse error `{:?}`", self.id, err);
                        continue;
                    }
                    Err(de::Error::IoError(err)) => {
                        println!("Manager {} : IoError `{}`", self.id, err);
                        Err(de::Error::IoError(err))
                    }
                };

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
