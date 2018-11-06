use bytes::{BufMut, BytesMut};
use ron::{de, ser};
use tokio::codec::{Decoder, Encoder};

use balthmessage::{Error, Message};

// TODO: As parameters...
const MESSAGE_SIZE_LIMIT: usize = 2 << 20;
// TODO: Is there a window between JOB_SIZE_LIMIT converted and MESSAGE_SIZE_LIMIT?
const JOB_SIZE_LIMIT: usize = MESSAGE_SIZE_LIMIT >> 2;

#[derive(Default)]
pub struct MessageCodec {
    msg_size: Option<usize>,
}

impl MessageCodec {
    pub fn new() -> Self {
        MessageCodec { msg_size: None }
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.msg_size.is_none() {
            if buf.len() >= 4 {
                let size_buf = buf.split_to(4);
                let size_array = [size_buf[0], size_buf[1], size_buf[2], size_buf[3]];
                self.msg_size = Some(u32::from_le_bytes(size_array) as usize);
            } else {
                return Ok(None);
            }
        }

        if let Some(msg_size) = self.msg_size {
            if buf.len() < msg_size {
                return Ok(None);
            } else {
                let msg = buf.split_to(msg_size);

                self.msg_size = None;

                // TODO: General method to parse msg buffer, ...
                let msg_res: de::Result<Message> = de::from_bytes(&msg[..]);

                let msg = msg_res?;

                // TODO: same with Task ?
                match msg {
                    Message::Job(job_id, _) => println!("received Job #{}.", job_id),
                    Message::Task(job_id, task_id, _) => {
                        println!("received Task #{} for Job #{}.", task_id, job_id)
                    }
                    Message::ReturnValue(job_id, task_id, _) => {
                        println!("received result for Task #{} for Job #{}.", task_id, job_id)
                    }
                    _ => println!("received `{:?}`.", msg),
                }

                Ok(Some(msg))
            }
        } else {
            Ok(None)
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
        if let Message::Job(_, bytecode) = &message {
            if bytecode.len() >= JOB_SIZE_LIMIT as usize {
                return Err(Error::JobTooBig(bytecode.len()));
            }
        }

        let msg_str = ser::to_string(&message)?;
        let len = msg_str.len();

        match message {
            Message::Job(job_id, _) => println!("sending Job #{} of {} bytes.", job_id, len),
            Message::Task(job_id, task_id, _) => println!(
                "sending Task #{} for Job #{} of {} bytes.",
                task_id, job_id, len
            ),
            Message::ReturnValue(job_id, task_id, _) => println!(
                "sending result for Task #{} for Job #{} of {} bytes.",
                task_id, job_id, len
            ),
            _ => println!("sending `{}` of {} bytes.", msg_str, len),
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
