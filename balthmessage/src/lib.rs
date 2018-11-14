#![feature(int_to_from_bytes)]
#![feature(vec_resize_default)]

#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;

extern crate balthajob as job;

pub mod mock_stream;

pub use ron::{de, ser};
use std::io;
use std::io::prelude::*;
use std::iter::FusedIterator;

use job::task::arguments::Arguments;

// TODO: unify with balthernet
type Pid=u32;
type ConnVote=u32;

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

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Hash)]
pub enum Message {
    Hello(String),
    // TODO: use `type Pid` and `type ConnVote`
    Connect(Pid),
    // TODO: rename ?
    ConnectReceived(Pid),
    Vote(ConnVote),
    ConnectAck,
    ConnectCancel,
    Connected(usize),
    // TODO: Useful to annouce deconnection ?
    Disconnect,
    Disconnected(usize),
    MessageTooBig,
    Idle(usize),
    RequestJob(usize),
    InvalidJobId(usize),
    Job(usize, Vec<u8>), // TODO: Job ids?
    JobRegisteredAt(usize),
    Task(usize, usize, Arguments), // TODO: Real type
    // TODO: or tasks?
    ReturnValue(usize, usize, Result<Arguments, ()>), // TODO: proper error
    // External(E) // TODO: generic type
    NoJob,
    TestBig(Vec<u8>),
    // TODO: Ping/Pong with Instants ? (latency, ...)
    Ping, // (Instant),
    Pong, // (Instant),
}

impl Message {
    pub fn send<W: Write>(&self, pode_id: usize, writer: &mut W) -> Result<(), Error> {
        // This prevents spending time to convert the task... :
        // TODO: same with Task ?
        if let Message::Job(_, bytecode) = self {
            if bytecode.len() >= JOB_SIZE_LIMIT as usize {
                return Err(Error::JobTooBig(bytecode.len()));
            }
        }

        let msg_str = ser::to_string(self)?;
        let len = msg_str.len();

        match self {
            Message::Job(job_id, _) => {
                println!("{} : sending Job #{} of {} bytes.", pode_id, job_id, len)
            }
            Message::Task(job_id, task_id, _) => println!(
                "{} : sending Task #{} for Job #{} of {} bytes.",
                pode_id, task_id, job_id, len
            ),
            Message::ReturnValue(job_id, task_id, _) => println!(
                "{} : sending result for Task #{} for Job #{} of {} bytes.",
                pode_id, task_id, job_id, len
            ),
            _ => println!("{} : sending `{}` of {} bytes.", pode_id, msg_str, len),
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

    // TODO: use std::io::Error s ?
    // TODO: tests
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
                Ok(Message::Disconnected(_)) => false,
                _ => true,
            })
            .map(|msg_res| -> Result<(), Error> {
                match msg_res {
                    Ok(msg) => closure(msg),
                    Err(err) => Err(err),
                }
            })
            .skip_while(|result| result.is_ok())
            .next();

        match res {
            Some(Err(err)) => Err(err),
            _ => {
                println!("{} : Disconnected socket.", id);
                Ok(())
            }
        }
    }
}

impl<R: Read> FusedIterator for MessageReader<R> {}

impl<R: Read> Iterator for MessageReader<R> {
    type Item = Result<Message, Error>;

    // TODO: clean this mess... (multiple returns ...)
    // TODO: use std::io::Error s ?
    fn next(&mut self) -> Option<Result<Message, Error>> {
        if let Some(mut reader) = self.reader.take() {
            let msg_size: usize = {
                let mut buffer: [u8; 4] = [0; 4];

                match reader.read_exact(&mut buffer) {
                    Ok(_) => {}
                    Err(err) => {
                        return match err.kind() {
                            io::ErrorKind::UnexpectedEof => None,
                            _ => Some(Err(Error::from(err))),
                        }
                    }
                };

                u32::from_le_bytes(buffer) as usize
            };

            if msg_size > MESSAGE_SIZE_LIMIT {
                // TODO: notify sender ?
                return Some(Err(Error::MessageTooBig(msg_size)));
            }

            // println!("{} : Receiving {} bytes...", self.id, msg_size);

            let mut buffer: Vec<u8> = Vec::new();
            buffer.resize_default(msg_size);

            // Loops until it has the full message:
            // TODO: use read_exact ?
            match reader.read_exact(&mut buffer) {
                Ok(n) => n,
                Err(err) => return Some(Err(Error::from(err))),
            };

            let msg_res: de::Result<Message> = de::from_bytes(&buffer.as_slice());
            let res = match msg_res {
                Ok(msg) => {
                    // TODO: same with Task ?
                    match msg {
                        Message::Job(job_id, _) => {
                            println!("{} : received Job #{}.", self.id, job_id)
                        }
                        Message::Task(job_id, task_id, _) => println!(
                            "{} : received Task #{} for Job #{}.",
                            self.id, task_id, job_id
                        ),
                        Message::ReturnValue(job_id, task_id, _) => println!(
                            "{} : received result for Task #{} for Job #{}.",
                            self.id, task_id, job_id
                        ),
                        _ => println!("{} : received `{:?}`.", self.id, msg),
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
    use super::*;
    use mock_stream::*;

    // ------------------------------------------------------------------
    // Message::send(...) :
    mod message_send {
        use super::*;

        fn test_send_msg(msg: Message) -> Result<(), Error> {
            let mut writer = MockStream::new();

            let res = msg.send(&mut writer);

            match res {
                Ok(()) => {
                    let msg_str = ser::to_string(&msg).unwrap();
                    let msg_len = msg_str.len();

                    assert_eq!(writer.bytes.len(), 4 + msg_len);

                    assert_eq!(&writer.bytes[..4], (msg_len as u32).to_le_bytes());
                    assert_eq!(String::from_utf8_lossy(&writer.bytes[4..]), msg_str);

                    Ok(())
                }
                Err(err) => Err(err),
            }
        }

        #[test]
        fn it_sends_a_small_msg() {
            let msg = Message::NoJob;

            if let Err(err) = test_send_msg(msg) {
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

            if let Err(err) = test_send_msg(msg) {
                panic!("Returned an error: {:?}", err);
            }
        }
        #[test]
        // #[ignore]
        fn it_doesnt_send_a_huge_msg() {
            let job_size = MESSAGE_SIZE_LIMIT << 1;

            let mut vec = Vec::with_capacity(job_size);
            unsafe {
                vec.set_len(job_size);
            }

            let msg = Message::TestBig(vec);

            match test_send_msg(msg) {
                Ok(_) => panic!("Didn't return an error!"),
                Err(Error::MessageTooBig(_)) => {}
                Err(err) => panic!("Didn't return MessageTooBig, returned : {:?}", err),
            }
        }
        #[test]
        fn it_doesnt_send_a_huge_job() {
            let job_id = 10;
            let task_id = 1246;
            let job_size = JOB_SIZE_LIMIT << 1;

            let mut vec = Vec::with_capacity(job_size);
            unsafe {
                vec.set_len(job_size);
            }

            let msg = Message::Job(job_id, task_id, vec);

            match test_send_msg(msg) {
                Ok(_) => panic!("Didn't return an error!"),
                Err(Error::JobTooBig(returned_size)) => assert_eq!(returned_size, job_size),
                Err(err) => panic!("Didn't return JobTooBig, returned : {:?}", err),
            }
        }
    }

    // ------------------------------------------------------------------
    // MessageReader::next() :
    mod message_reader_next {
        use super::*;

        fn test_receive_msg(
            mut given_stream: Option<&mut MockStream>,
            ref_msg: Message,
        ) -> Result<Message, Error> {
            let mut stream = MockStream::new();
            ref_msg.send(&mut stream).unwrap();

            let stream = match given_stream.take() {
                Some(stream) => stream,
                None => &mut stream,
            };
            let mut reader = MessageReader::new(0, stream);

            match reader.next() {
                Some(Ok(read_msg)) => {
                    assert_eq!(read_msg, ref_msg);
                    Ok(read_msg)
                }
                Some(Err(err)) => Err(err),
                None => panic!("Didn't receive any message!"),
            }
        }

        #[test]
        fn it_receives_a_small_msg() {
            let msg = Message::NoJob;

            if let Err(err) = test_receive_msg(None, msg) {
                panic!("Received an error : {:?}", err);
            }
        }
        #[test]
        fn it_receives_a_big_msg() {
            let job_size = MESSAGE_SIZE_LIMIT >> 5;
            let mut vec = Vec::with_capacity(job_size);
            unsafe {
                vec.set_len(job_size);
            }

            let msg = Message::TestBig(vec.clone());

            match test_receive_msg(None, msg) {
                Ok(Message::TestBig(received_vec)) => assert_eq!(received_vec, vec),
                Ok(msg) => panic!("Didn't receive the right message : {:?}", msg),
                Err(err) => panic!("Received an error : {:?}", err),
            }
        }
        #[test]
        fn it_doesnt_receive_a_huge_msg() {
            let job_size = MESSAGE_SIZE_LIMIT << 1;
            let job_size_buffer = u32::to_le_bytes(job_size as u32);

            let mut stream = MockStream::new();
            stream.write(&job_size_buffer[..]).unwrap();

            let mut reader = MessageReader::new(0, stream);

            match reader.next() {
                Some(Ok(msg)) => panic!("Received a message : {:?}", msg),
                Some(Err(Error::MessageTooBig(received_size))) => {
                    assert_eq!(job_size, received_size)
                }
                Some(Err(err)) => panic!("Didn't received the correct error : {:?}", err),
                None => panic!("Didn't receive any message!"),
            }
        }
        #[test]
        fn it_receives_none_when_connection_closed() {
            let stream = MockStream::new();
            let mut reader = MessageReader::new(0, stream);

            match reader.next() {
                Some(Ok(msg)) => panic!("Received a message : {:?}", msg),
                Some(Err(err)) => panic!("Received an error : {:?}", err),
                None => {}
            }
        }
        #[test]
        fn it_receives_several_messages() {
            let mut stream = MockStream::new();

            let mut msgs = vec![
                Message::NoJob,
                Message::MessageTooBig,
                Message::Hello(String::from("this is a test")),
            ];
            msgs.iter().for_each(|m| m.send(&mut stream).unwrap());

            msgs.drain(..).for_each(|m| {
                if let Err(err) = test_receive_msg(Some(&mut stream), m) {
                    panic!("Received an error : {:?}", err);
                }
            });
        }
        // TODO: fuzzy testing
    }
}
