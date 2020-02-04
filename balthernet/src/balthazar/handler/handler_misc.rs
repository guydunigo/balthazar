//! This module is greatly inspired from [`libp2p::kad::handler::KademliaHandler`].
//!
//! Different classes used by the [`Balthandler`](`super::Balthandler`).
use libp2p::swarm::ProtocolsHandlerUpgrErr;
use proto::protobuf::ProtoBufProtocolSink;
use std::{error, fmt, io};

/// Unique identifier for a connection.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct UniqueConnecId(u64);

impl UniqueConnecId {
    /// Increments the contained Id.
    pub fn inc(&self) -> UniqueConnecId {
        UniqueConnecId(self.0 + 1)
    }
}

/// Unique identifier for a request. Must be passed back in order to answer a request from
/// the remote.
///
/// We don't implement `Clone` on purpose, in order to prevent users from answering the same
/// request twice.
#[derive(Debug, PartialEq, Eq)]
pub struct RequestId {
    /// Unique identifier for an incoming connection.
    connec_unique_id: UniqueConnecId,
}

impl RequestId {
    pub fn new(request_id: UniqueConnecId) -> Self {
        RequestId {
            connec_unique_id: request_id,
        }
    }

    pub fn connec_unique_id(&self) -> &UniqueConnecId {
        &self.connec_unique_id
    }
}

/// Error that can happen when handling a query.
#[derive(Debug)]
pub enum BalthandlerQueryErr {
    /// Error while trying to perform the query.
    Upgrade(ProtocolsHandlerUpgrErr<io::Error>),
    /// Received an answer that doesn't correspond to the request.
    UnexpectedMessage,
    /// I/O error in the substream.
    Io(io::Error),
}

impl error::Error for BalthandlerQueryErr {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            BalthandlerQueryErr::Upgrade(err) => Some(err),
            BalthandlerQueryErr::UnexpectedMessage => None,
            BalthandlerQueryErr::Io(err) => Some(err),
        }
    }
}

impl fmt::Display for BalthandlerQueryErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BalthandlerQueryErr::Upgrade(err) => write!(f, "Error while performing query: {}", err),
            BalthandlerQueryErr::UnexpectedMessage => {
                write!(f, "Remote answered our query with the wrong message type")
            }
            BalthandlerQueryErr::Io(err) => write!(f, "I/O error during a query: {}", err),
        }
    }
}

impl From<ProtocolsHandlerUpgrErr<io::Error>> for BalthandlerQueryErr {
    #[inline]
    fn from(err: ProtocolsHandlerUpgrErr<io::Error>) -> Self {
        BalthandlerQueryErr::Upgrade(err)
    }
}

impl From<io::Error> for BalthandlerQueryErr {
    #[inline]
    fn from(err: io::Error) -> Self {
        BalthandlerQueryErr::Io(err)
    }
}

/// State of an active substream, opened either by us or by the remote.
pub enum SubstreamState<TMessage, TUserData> {
    /// We haven't started opening the outgoing substream yet.
    /// Contains the request we want to send, and the user data if we expect an answer.
    OutPendingOpen(TMessage, Option<TUserData>),
    /// Waiting to send a message to the remote.
    OutPendingSend(ProtoBufProtocolSink<TMessage>, TMessage, Option<TUserData>),
    /// Waiting to flush the substream so that the data arrives to the remote.
    OutPendingFlush(ProtoBufProtocolSink<TMessage>, Option<TUserData>),
    /// Waiting for an answer back from the remote.
    // TODO: add timeout (comment from the Kadmelia implementation)
    OutWaitingAnswer(ProtoBufProtocolSink<TMessage>, TUserData),
    /// An error happened on the substream and we should report the error to the user.
    OutReportError(BalthandlerQueryErr, TUserData),
    /// The substream is being closed.
    OutClosing(ProtoBufProtocolSink<TMessage>),
    /// Waiting for a request from the remote.
    InWaitingMessage(UniqueConnecId, ProtoBufProtocolSink<TMessage>),
    /// Waiting for the user to send a `KademliaHandlerIn` event containing the response.
    InWaitingUser(UniqueConnecId, ProtoBufProtocolSink<TMessage>),
    /// Waiting to send an answer back to the remote.
    InPendingSend(UniqueConnecId, ProtoBufProtocolSink<TMessage>, TMessage),
    /// Waiting to flush an answer back to the remote.
    InPendingFlush(UniqueConnecId, ProtoBufProtocolSink<TMessage>),
    /// The substream is being closed.
    InClosing(ProtoBufProtocolSink<TMessage>),
}

/*
impl<TMessage, TUserData> SubstreamState<TMessage, TUserData> {
    /// Tries to close the substream.
    ///
    /// If the substream is not ready to be closed, returns it back.
    fn try_close(&mut self, cx: &mut Context) -> Poll<()> {
        match self {
            SubstreamState::OutPendingOpen(_, _) | SubstreamState::OutReportError(_, _) => {
                Poll::Ready(())
            }
            SubstreamState::OutPendingSend(ref mut stream, _, _)
            | SubstreamState::OutPendingFlush(ref mut stream, _)
            | SubstreamState::OutWaitingAnswer(ref mut stream, _)
            | SubstreamState::OutClosing(ref mut stream) => {
                match Sink::poll_close(Pin::new(stream), cx) {
                    Poll::Ready(_) => Poll::Ready(()),
                    Poll::Pending => Poll::Pending,
                }
            }
            SubstreamState::InWaitingMessage(_, ref mut stream)
            | SubstreamState::InWaitingUser(_, ref mut stream)
            | SubstreamState::InPendingSend(_, ref mut stream, _)
            | SubstreamState::InPendingFlush(_, ref mut stream)
            | SubstreamState::InClosing(ref mut stream) => {
                match Sink::poll_close(Pin::new(stream), cx) {
                    Poll::Ready(_) => Poll::Ready(()),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}
*/
