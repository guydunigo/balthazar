//! Generic **libp2p protocol** ([`ProtoBufProtocol`]) and
//! codecs ([`ProtoBufCodec`] and [`ProtoBufLengthCodec`]) for protobuf.
use futures::future;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{sink::Sink, Stream};
use futures_codec::Framed;
use libp2p::core::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use prost::Message;
use std::{io, iter};

mod codec;
pub use codec::*;

/// Generic trait for the protocols [`Sink`] and [`Stream`] for [`InboundUpgrade`] and
/// [`OutboundUpgrade`].
pub trait SinkStream<M, E>: Sink<M, Error = E> + Stream<Item = Result<M, E>> {}
impl<M, E, T: Sink<M, Error = E> + Stream<Item = Result<M, E>>> SinkStream<M, E> for T {}

/// Generic type for the protocols [`Sink`] and [`Stream`] for [`InboundUpgrade`] and
/// [`OutboundUpgrade`].
pub type ProtoBufProtocolSink<M> = Box<dyn SinkStream<M, io::Error> + Send + Unpin>;

/// Codec used by the [`ProtoBufProtocol`].
///
/// To be noted that the [`ProtoBufCodec`] doesn't seem to work as it doesn't know the size to receive...
/// **Use [`ProtoBufLengthCodec`] instead**,
type Codec<M> = ProtoBufLengthCodec<M>;

/// Generic protocol used with protobuf messages.
#[derive(Clone, Debug)]
pub struct ProtoBufProtocol<M> {
    protocol_version: &'static [u8],
    _marker: std::marker::PhantomData<M>,
}

impl<M> ProtoBufProtocol<M> {
    pub fn new(protocol_version: &'static [u8]) -> Self {
        ProtoBufProtocol {
            protocol_version,
            _marker: Default::default(),
        }
    }
}

impl<M: Default> UpgradeInfo for ProtoBufProtocol<M> {
    type Info = &'static [u8];
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(self.protocol_version)
    }
}

impl<TSocket, M> InboundUpgrade<TSocket> for ProtoBufProtocol<M>
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    M: Message + Default + 'static,
{
    type Output = ProtoBufProtocolSink<M>;
    type Future = future::Ready<Result<Self::Output, Self::Error>>;
    type Error = io::Error;

    fn upgrade_inbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        // eprintln!("Upgrade inbound");
        let codec = Codec::new();
        future::ok(Box::new(Framed::new(socket, codec)))
    }
}

impl<TSocket, M> OutboundUpgrade<TSocket> for ProtoBufProtocol<M>
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    M: Message + Default + 'static,
{
    type Output = ProtoBufProtocolSink<M>;
    type Future = future::Ready<Result<Self::Output, Self::Error>>;
    type Error = io::Error;

    fn upgrade_outbound(self, socket: TSocket, _: Self::Info) -> Self::Future {
        // eprintln!("Upgrade outbound");
        let codec = Codec::new();
        future::ok(Box::new(Framed::new(socket, codec)))
    }
}
