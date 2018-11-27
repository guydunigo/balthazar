#![feature(vec_resize_default)]

#[macro_use]
extern crate serde_derive;
extern crate bytes;
extern crate ron;
extern crate serde;
extern crate tokio;

extern crate balthajob as job;

mod codec;
mod message;
pub mod mock_stream;

pub use ron::{de, ser};
use std::fmt;
use std::io;

pub use codec::ProtoCodec;
use job::task::arguments::Arguments;
use job::task::TaskId;
use job::JobId;
pub use message::Message;

// TODO: unify with balthernet
type PeerId = u32;
type ConnVote = u32;
#[allow(dead_code)]
type PodeId = u32;
type Nonce = u128;

// ------------------------------------------------------------------
// Errors

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    SerError(ser::Error),
    DeError(de::Error),
    CouldNotGetSize,
    PacketTooBig(usize),
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
// Proto

/// Base proto messages between to network layers.
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Hash)]
pub enum Proto {
    MessageTooBig,
    Connect(PeerId),
    Vote(ConnVote),
    ConnectAck,
    ConnectCancel,
    Connected(usize),
    // TODO: Ping/Pong with Instants ? (latency, ...)
    Ping, // (Instant),
    Pong, // (Instant),
    /// Broadcast(route_list, msg)
    /// The original sender being the first of `route_list`
    Broadcast(Vec<PeerId>, M),
    /// ForwardTo(to, route_list, msg)
    /// The sender being the first of `route_list`.
    ForwardTo(PeerId, Vec<PeerId>, M),
    TestBig(Vec<u8>),
    Direct(M),
}

impl fmt::Display for Proto {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Proto::*;

        match self {
            Broadcast(_, m) => write!(f, "Broadcast : {}", m),
            ForwardTo(to, _, m) => write!(f, "Forward : to `{}` : {}", to, m),
            Direct(m) => write!(f, "Direct : {}", m),
            TestBig(v) => write!(f, "TestBig of {} bytes", v.len()),
            _ => {
                let debug = format!("{:?}", self);
                write!(f, "{}", &debug[..])
            }
        }
    }
}

impl Proto {
    /// Get the inner message of the packet if any or return `None`.
    pub fn get_message(&self) -> Option<&M> {
        match self {
            Proto::Broadcast(_, m) => Some(m),
            Proto::ForwardTo(_, _, m) => Some(m),
            Proto::Direct(m) => Some(m),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------
//

// TODO: rename ?
/// Packaged message with some "metadata".
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Hash)]
pub struct M {
    nonce: Nonce,
    msg: Message,
    // TODO: later signature, ...
}

impl fmt::Display for M {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.msg)
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

            let res = msg.send(0, &mut writer);

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

            let msg = Message::Job(0, job_id, vec);

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
            ref_msg.send(0, &mut stream).unwrap();

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
            msgs.iter().for_each(|m| m.send(0, &mut stream).unwrap());

            msgs.drain(..).for_each(|m| {
                if let Err(err) = test_receive_msg(Some(&mut stream), m) {
                    panic!("Received an error : {:?}", err);
                }
            });
        }
        // TODO: fuzzy testing
    }
}
