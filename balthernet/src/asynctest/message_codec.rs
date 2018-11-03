use bytes::BytesMut;
use ron::{de, ser};
use tokio::codec::{Decoder, Encoder};
use tokio::io;

use balthmessage as message;
use balthmessage::Message;

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
    type Error = message::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let None = self.msg_size {
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
    type Error = message::Error;

    fn encode(&mut self, message: Message, buf: &mut BytesMut) -> Result<(), message::Error> {
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
        Ok(())
    }
}
