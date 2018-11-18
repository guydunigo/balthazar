use bytes::{BufMut, BytesMut};
use ron::{de, ser};
use tokio::codec::{Decoder, Encoder};

use super::PeerId;
use balthmessage::{Error, Message};

// TODO: As parameters...
const MESSAGE_SIZE_LIMIT: usize = 2 << 20;
// TODO: Is there a window between JOB_SIZE_LIMIT converted and MESSAGE_SIZE_LIMIT?
const JOB_SIZE_LIMIT: usize = MESSAGE_SIZE_LIMIT >> 2;
/// Size of the header containing the message size in bytes.
const MESSAGE_SIZE_SIZE: usize = 4;

#[derive(Default)]
pub struct MessageCodec {
    peer_pid: Option<PeerId>,
}

impl MessageCodec {
    pub fn new(peer_pid: Option<PeerId>) -> Self {
        MessageCodec { peer_pid }
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let msg_size = if buf.len() >= MESSAGE_SIZE_SIZE {
            let size_array = [buf[0], buf[1], buf[2], buf[3]];
            u32::from_le_bytes(size_array) as usize
        } else {
            return Ok(None);
        };

        if buf.len() < msg_size + MESSAGE_SIZE_SIZE {
            return Ok(None);
        } else {
            buf.split_to(MESSAGE_SIZE_SIZE);
            let msg = buf.split_to(msg_size);

            // TODO: General method to parse msg buffer, ...
            let msg_res: de::Result<Message> = de::from_bytes(&msg[..]);

            let msg = msg_res?;

            // TODO: same with Task ?
            match msg {
                Message::Job(_, job_id, _) => println!("-- Received : Job #{}.", job_id),
                Message::Task(job_id, task_id, _) => println!(
                    "-- {:?} : Received : Task #{} for Job #{}.",
                    self.peer_pid, task_id, job_id
                ),
                Message::ReturnValue(job_id, task_id, _) => println!(
                    "-- {:?} : Received : result for Task #{} for Job #{}.",
                    self.peer_pid, task_id, job_id
                ),
                Message::Ping | Message::Pong => (),
                Message::Idle(_) | Message::NoJob => (),
                _ => println!("-- {:?} : Received : `{:?}`.", self.peer_pid, msg),
            }

            Ok(Some(msg))
        }
    }
}

impl Encoder for MessageCodec {
    type Item = Message;
    type Error = Error;

    fn encode(&mut self, message: Message, buf: &mut BytesMut) -> Result<(), Error> {
        /*
        // It's important to reserve the amount of space needed. The `bytes` API
        // does not grow the buffers implicitly.
        // Reserve the length of the string + 1 for the '\n'.
        buf.reserve(line.len() + 1);
        
        // String implements IntoBuf, a trait used by the `bytes` API to work with
        // types that can be expressed as a sequence of bytes.
        buf.put(line);
        
        // Put the '\n' in the buffer.
        buf.put_u8(b'\n');
        
        // Return ok to signal that no error occured.
        */
        // This prevents spending time to convert the task... :
        // TODO: same with Task ?
        if let Message::Job(_, _, bytecode) = &message {
            if bytecode.len() >= JOB_SIZE_LIMIT as usize {
                return Err(Error::JobTooBig(bytecode.len()));
            }
        }

        let msg_str = ser::to_string(&message)?;
        let len = msg_str.len();

        match message {
            Message::Job(_, job_id, _) => println!(
                "-- {:?} : Sending Job #{} of {} bytes.",
                self.peer_pid, job_id, len
            ),
            Message::Task(job_id, task_id, _) => println!(
                "-- {:?} : Sending Task #{} for Job #{} of {} bytes.",
                self.peer_pid, task_id, job_id, len
            ),
            Message::ReturnValue(job_id, task_id, _) => println!(
                "-- {:?} : Sending result for Task #{} for Job #{} of {} bytes.",
                self.peer_pid, task_id, job_id, len
            ),
            Message::Ping | Message::Pong => (),
            Message::Idle(_) | Message::NoJob => (),
            _ => println!(
                "-- {:?} : Sending : `{}` of {} bytes.",
                self.peer_pid, msg_str, len
            ),
        }

        if len >= MESSAGE_SIZE_LIMIT as usize {
            return Err(Error::MessageTooBig(len));
        }

        buf.reserve(len - buf.len() + 4);
        buf.put_u32_le(len as u32);
        buf.put_slice(msg_str.as_bytes());

        Ok(())
    }
}
