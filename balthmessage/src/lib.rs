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

// TODO: As parameters...
const MESSAGE_SIZE_LIMIT: usize = 2 << 20;
// TODO: Is there a window between JOB_SIZE_LIMIT converted and MESSAGE_SIZE_LIMIT?
const JOB_SIZE_LIMIT: usize = MESSAGE_SIZE_LIMIT >> 2;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    SerError(ser::Error),
    DeError(de::Error),
    CouldNotGetSize,
    MessageTooBig(usize),
    JobTooBig(usize),
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
// Message

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Hello(String),
    Connected(usize),
    // TODO: Useful to annouce deconnection ?
    Disconnect,
    Disconnected(usize),
    MessageTooBig,
    Idle(usize),
    Job(usize, usize, Vec<u8>),                     // TODO: Job ids?
    ReturnValue(usize, usize, Result<Vec<u8>, ()>), // TODO: proper error
    // External(E) // TODO: generic type
    NoJob,
    TestBig(Vec<u8>),
}

impl Message {
    pub fn send<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        // This prevents spending time to convert the task... :
        if let Message::Job(_, _, bytecode) = self {
            if bytecode.len() >= JOB_SIZE_LIMIT as usize {
                return Err(Error::JobTooBig(bytecode.len()));
            }
        }

        let msg_str = ser::to_string(self)?;
        let len = msg_str.len();

        if let Message::Job(job_id, task_id, _) = self {
            println!(
                "sending Task #{} of Job #{} of {} bytes.",
                task_id, job_id, len
            );
        } else {
            println!("sending `{}` of {} bytes.", msg_str, len);
        }

        if len >= MESSAGE_SIZE_LIMIT as usize {
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

            if msg_size > MESSAGE_SIZE_LIMIT {
                // TODO: notify sender ?
                return Some(Err(Error::MessageTooBig(msg_size)));
            }

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
                    if let Message::Job(job_id, task_id, _) = msg {
                        println!(
                            "{} : received Task #{} of Job #{}.",
                            self.id, task_id, job_id
                        );
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

// ------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::Error;
    use super::Message;
    use super::JOB_SIZE_LIMIT;
    use super::MESSAGE_SIZE_LIMIT;
    use ron::{de, ser};
    use std::io::prelude::*;

    // ------------------------------------------------------------------

    struct MockWriter {
        bytes: Vec<u8>,
        len: usize,
    }
    impl MockWriter {
        pub fn new() -> MockWriter {
            MockWriter {
                bytes: Vec::new(),
                len: 0,
            }
        }
    }
    impl Write for MockWriter {
        fn write(&mut self, bytes: &[u8]) -> std::result::Result<usize, std::io::Error> {
            bytes.iter().for_each(|b| self.bytes.push(b.clone()));
            self.len += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::result::Result<(), std::io::Error> {
            unimplemented!();
        }
    }

    fn test_send_msg(msg: Message) -> (Result<(), Error>, usize) {
        let mut writer = MockWriter::new();

        let msg_str = ser::to_string(&msg).unwrap();
        let msg_len = msg_str.len();

        let res = msg.send(&mut writer);

        (
            match res {
                Ok(()) => {
                    assert_eq!(writer.len, 4 + msg_len);

                    assert_eq!(&writer.bytes[..4], (msg_len as u32).to_le_bytes());
                    assert_eq!(String::from_utf8_lossy(&writer.bytes[4..]), msg_str);

                    Ok(())
                }
                Err(err) => Err(err),
            },
            msg_len,
        )
    }

    // ------------------------------------------------------------------

    #[test]
    fn it_sends_a_small_msg() {
        let msg = Message::NoJob;

        if let Err(err) = test_send_msg(msg).0 {
            panic!("Returned an error: {:?}", err);
        }
    }
    #[test]
    fn it_sends_a_big_msg() {
        let job_size = MESSAGE_SIZE_LIMIT >> 5;

        let mut vec = Vec::with_capacity(job_size);
        unsafe {
            vec.set_len(job_size);
        }

        let msg = Message::TestBig(vec);

        if let Err(err) = test_send_msg(msg).0 {
            panic!("Returned an error: {:?}", err);
        }
    }
    #[test]
    #[ignore]
    fn it_shouldnt_send_a_huge_msg() {
        let job_size = MESSAGE_SIZE_LIMIT << 1;

        let mut vec = Vec::with_capacity(job_size);
        unsafe {
            vec.set_len(job_size);
        }

        let msg = Message::TestBig(vec);

        let (res, msg_len) = test_send_msg(msg);

        match res {
            Ok(_) => panic!("Didn't return an error!"),
            Err(Error::MessageTooBig(returned_len)) => assert_eq!(returned_len, msg_len),
            Err(err) => panic!("Didn't return MessageTooBig, returned : {:?}", err),
        }
    }
    #[test]
    fn it_shouldnt_send_a_huge_job() {
        let job_id = 10;
        let task_id = 1246;
        let job_size = JOB_SIZE_LIMIT << 1;

        let mut vec = Vec::with_capacity(job_size);
        unsafe {
            vec.set_len(job_size);
        }

        let msg = Message::Job(job_id, task_id, vec);

        let (res, _) = test_send_msg(msg);

        match res {
            Ok(_) => panic!("Didn't return an error!"),
            Err(Error::JobTooBig(returned_size)) => assert_eq!(returned_size, job_size),
            Err(err) => panic!("Didn't return JobTooBig, returned : {:?}", err),
        }
    }
}
