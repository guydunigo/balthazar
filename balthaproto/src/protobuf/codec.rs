//! Some generic codecs to use with **Protobuf** messages.
use bytes::BytesMut;
use futures_codec::{Decoder, Encoder /*, LengthCodec*/};
use prost::Message;
use std::io;

pub type ProtoBufLengthCodec<M> = ProtoBufCodec<M>;

/// Generic codec for **protobuf** messages.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProtoBufCodec<M> {
    _marker: std::marker::PhantomData<M>,
}

impl<M: Default> ProtoBufCodec<M> {
    pub fn new() -> Self {
        ProtoBufCodec::default()
    }
}

impl<M> Decoder for ProtoBufCodec<M>
where
    M: Message + Default,
{
    type Item = M;
    type Error = io::Error;

    // TODO: reliable to shut up all errors ?
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let answer = Self::Item::decode_length_delimited(src)
            .map(Some)
            .unwrap_or_else(|_| None);
        Ok(answer)
        // .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

impl<M> Encoder for ProtoBufCodec<M>
where
    M: Message,
{
    type Item = M;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.encode_length_delimited(dst)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(())
    }
}

/*
/// Prefixes the [`ProtoBufCodec`] with a `u64` indicating the size of the message using
/// [`LengthCodec`].
pub struct ProtoBufLengthCodec<M> {
    length_codec: LengthCodec,
    protobuf_codec: ProtoBufCodec<M>,
    _marker: std::marker::PhantomData<M>,
}

impl<M: Default> Default for ProtoBufLengthCodec<M> {
    fn default() -> Self {
        ProtoBufLengthCodec {
            length_codec: LengthCodec,
            protobuf_codec: ProtoBufCodec::default(),
            _marker: Default::default(),
        }
    }
}

impl<M> Decoder for ProtoBufLengthCodec<M>
where
    M: Message + Default,
{
    type Item = M;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // eprintln!("decode {:?} {}", src, src.len());
        if let Some(buf) = self.length_codec.decode(src)? {
            // TODO: better way to convert Bytes to BytesMut ?
            let mut bufmut = BytesMut::with_capacity(buf.len());
            bufmut.extend_from_slice(&buf[..]);

            if let Some(item) = self.protobuf_codec.decode(&mut bufmut)? {
                // eprintln!("decoded {:?}", item);
                Ok(Some(item))
            } else {
                // eprintln!("decoded message none");
                Ok(None)
            }
        } else {
            // eprintln!("decoded length none");
            Ok(None)
        }
    }
}

impl<M> Encoder for ProtoBufLengthCodec<M>
where
    M: Message,
{
    type Item = M;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut tmp_dst = BytesMut::new();
        // eprintln!("length encode {:?}", item);
        self.protobuf_codec.encode(item, &mut tmp_dst)?;
        self.length_codec.encode(tmp_dst.into(), dst)?;
        // eprintln!("length encoded {:?} {}", dst, dst.len());
        Ok(())
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker;
    use bytes::BytesMut;

    #[test]
    fn it_encodes_decodes_with_protobuf_codec() {
        let original_msg: worker::WorkerMsgWrapper = worker::NotMine {}.into();
        let mut codec = ProtoBufCodec::default();

        let mut bytes = BytesMut::new();

        codec.encode(original_msg.clone(), &mut bytes).unwrap();
        let decoded_msg = codec.decode(&mut bytes).unwrap().unwrap();

        assert_eq!(original_msg, decoded_msg);
    }

    #[test]
    fn it_encodes_decodes_with_protobuf_length_codec() {
        let original_msg: worker::WorkerMsgWrapper = worker::NotMine {}.into();
        let mut codec = ProtoBufLengthCodec::default();

        let mut bytes = BytesMut::new();

        codec.encode(original_msg.clone(), &mut bytes).unwrap();
        let decoded_msg = codec.decode(&mut bytes).unwrap().unwrap();

        assert_eq!(original_msg, decoded_msg);
    }
}
