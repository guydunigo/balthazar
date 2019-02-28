use bytes::{BufMut, BytesMut};
use tokio::codec::{Decoder, Encoder};

use super::*;

// TODO: As parameters ?
const PACKET_SIZE_LIMIT: usize = 2 << 20;
// TODO: relevant ?
// const JOB_SIZE_LIMIT: usize = PACKET_SIZE_LIMIT >> 2;
/// Size of the header containing the message size in bytes.
/// If changed, change the corresponding variable type (`u32`).
const PACKET_SIZE_SIZE: usize = 4;

#[derive(Default)]
pub struct ProtoCodec {
    peer_pid: Option<PeerId>,
}

impl ProtoCodec {
    pub fn new(peer_pid: Option<PeerId>) -> Self {
        ProtoCodec { peer_pid }
    }
}

impl Decoder for ProtoCodec {
    type Item = Proto;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let pkt_size = if buf.len() >= PACKET_SIZE_SIZE {
            let mut size_array: [u8; PACKET_SIZE_SIZE] = [0; PACKET_SIZE_SIZE];
            for i in 0..PACKET_SIZE_SIZE {
                size_array[i] = buf[i];
            }
            u32::from_le_bytes(size_array) as usize
        } else {
            return Ok(None);
        };

        if buf.len() < pkt_size + PACKET_SIZE_SIZE {
            return Ok(None);
        } else {
            buf.split_to(PACKET_SIZE_SIZE);
            let pkt = buf.split_to(pkt_size);

            // TODO: General method to parse pkt buffer, ...
            let pkt_res: de::Result<Self::Item> = de::from_bytes(&pkt[..]);

            let pkt = pkt_res?;

            match pkt {
                Proto::Ping | Proto::Pong => (),
                Proto::ForwardTo(_, _, _) => (),
                Proto::Broadcast(_, _) => (),
                _ => println!("-- {:?} : Received : {}", self.peer_pid, pkt),
            }

            Ok(Some(pkt))
        }
    }
}

impl Encoder for ProtoCodec {
    type Item = Proto;
    type Error = Error;

    fn encode(&mut self, packet: Self::Item, buf: &mut BytesMut) -> Result<(), Self::Error> {
        /*
        // TODO: move to other plate (shoal.package ?)
        if let Proto::Job(_, _, bytecode) = &message {
            if bytecode.len() >= JOB_SIZE_LIMIT as usize {
                return Err(Error::JobTooBig(bytecode.len()));
            }
        }
        */

        let pkt_str = ser::to_string(&packet)?;
        let len = pkt_str.len();

        match packet {
            Proto::Ping | Proto::Pong => (),
            Proto::ForwardTo(_, _, _) => (),
            Proto::Broadcast(_, _) => (),
            _ => println!(
                "-- {:?} : Sending : `{}` of {} bytes.",
                self.peer_pid, packet, len
            ),
        }

        if len >= PACKET_SIZE_LIMIT as usize {
            return Err(Error::PacketTooBig(len));
        }

        buf.reserve(len + 4);
        buf.put_u32_le(len as u32);
        buf.put_slice(pkt_str.as_bytes());

        Ok(())
    }
}
