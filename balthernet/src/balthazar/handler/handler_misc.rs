//! Different classes/functions used by the [`Balthandler`](`super::Balthandler`).
//!
//! This module is greatly inspired from [`libp2p::kad::handler::KademliaHandler`].
use futures::{sink::Sink, Stream};
use libp2p::swarm::{ProtocolsHandlerEvent, ProtocolsHandlerUpgrErr, SubstreamProtocol};
use proto::protobuf::ProtoBufProtocol;
use proto::protobuf::ProtoBufProtocolSink;
use proto::worker::WorkerMsgWrapper;
use std::{
    error, fmt, io,
    pin::Pin,
    task::{Context, Poll},
};

use super::{process_answer, process_request, EventOut};

/// Unique identifier for a connection.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct UniqueConnecId(u64);

impl UniqueConnecId {
    /// Increments the contained Id.
    pub fn inc(self) -> UniqueConnecId {
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

    /// Returns a dummy RequestId.
    /// **Warning: This is probably non-unique and should be used only for logging and such!**
    pub fn dummy_dangerous() -> Self {
        RequestId::new(UniqueConnecId(0))
    }

    /// Returns a dummy RequestId.
    ///
    /// > **Warning: This is not actual cloning and should be avoided when possible!**
    /// > **This is probably non-unique and should be used only for logging and such!**
    pub fn clone_dangerous(&self) -> Self {
        Self::dummy_dangerous()
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

/// Advances one substream.
///
/// Returns the new state for that substream, an event to generate, and whether the substream
/// should be polled again.
pub fn advance_substream<TUserData>(
    state: SubstreamState<WorkerMsgWrapper, TUserData>,
    upgrade: ProtoBufProtocol<WorkerMsgWrapper>,
    cx: &mut Context,
) -> (
    // New substream if it needs to be used again:
    Option<SubstreamState<WorkerMsgWrapper, TUserData>>,
    // Answer event with the action to do (send ask for new substream, error, send event to
    // behaviour, ...):
    Option<
        ProtocolsHandlerEvent<
            ProtoBufProtocol<WorkerMsgWrapper>,
            (WorkerMsgWrapper, Option<TUserData>),
            EventOut<TUserData>,
            io::Error,
        >,
    >,
    // Should the substream be polled again:
    bool,
) {
    use SubstreamState::*;
    match state {
        OutPendingOpen(msg, user_data) => {
            let evt = ProtocolsHandlerEvent::OutboundSubstreamRequest {
                protocol: SubstreamProtocol::new(upgrade),
                info: (msg, user_data),
            };
            // println!("OutPendingOpen");
            (None, Some(evt), false)
        }
        OutPendingSend(mut substream, msg, user_data) => {
            // println!("OutPendingSend");
            match Sink::poll_ready(Pin::new(&mut substream), cx) {
                Poll::Ready(Ok(_)) => match Sink::start_send(Pin::new(&mut substream), msg) {
                    Ok(_) => (Some(OutPendingFlush(substream, user_data)), None, true),
                    Err(error) => {
                        let event = if let Some(user_data) = user_data {
                            Some(ProtocolsHandlerEvent::Custom(EventOut::QueryError {
                                error: BalthandlerQueryErr::Io(error),
                                user_data,
                            }))
                        } else {
                            None
                        };
                        (None, event, false)
                    }
                },
                Poll::Pending => (Some(OutPendingSend(substream, msg, user_data)), None, false),
                Poll::Ready(Err(error)) => {
                    let event = if let Some(user_data) = user_data {
                        Some(ProtocolsHandlerEvent::Custom(EventOut::QueryError {
                            error: BalthandlerQueryErr::Io(error),
                            user_data,
                        }))
                    } else {
                        None
                    };
                    (None, event, false)
                }
            }
        }
        OutPendingFlush(mut substream, user_data) => {
            // println!("OutPendingFlush");
            match Sink::poll_flush(Pin::new(&mut substream), cx) {
                Poll::Ready(Ok(())) => {
                    if let Some(user_data) = user_data {
                        (Some(OutWaitingAnswer(substream, user_data)), None, true)
                    } else {
                        (Some(OutClosing(substream)), None, true)
                    }
                }
                Poll::Pending => (Some(OutPendingFlush(substream, user_data)), None, false),
                Poll::Ready(Err(error)) => {
                    let event = if let Some(user_data) = user_data {
                        Some(ProtocolsHandlerEvent::Custom(EventOut::QueryError {
                            error: BalthandlerQueryErr::Io(error),
                            user_data,
                        }))
                    } else {
                        None
                    };

                    (None, event, false)
                }
            }
        }
        OutWaitingAnswer(mut substream, user_data) => {
            // println!("OutWaitingAnswer");
            match Stream::poll_next(Pin::new(&mut substream), cx) {
                Poll::Ready(Some(Ok(msg))) => {
                    let new_state = OutClosing(substream);
                    if let Some(event) = process_answer(msg, user_data) {
                        (
                            Some(new_state),
                            Some(ProtocolsHandlerEvent::Custom(event)),
                            true,
                        )
                    } else {
                        (Some(new_state), None, true)
                    }
                }
                Poll::Pending => (Some(OutWaitingAnswer(substream, user_data)), None, false),
                Poll::Ready(Some(Err(error))) => {
                    let event = EventOut::QueryError {
                        error: error.into(),
                        user_data,
                    };
                    (None, Some(ProtocolsHandlerEvent::Custom(event)), false)
                }
                Poll::Ready(None) => {
                    let event = EventOut::QueryError {
                        error: BalthandlerQueryErr::Io(io::ErrorKind::UnexpectedEof.into()),
                        user_data,
                    };
                    (None, Some(ProtocolsHandlerEvent::Custom(event)), false)
                }
            }
        }
        OutReportError(error, user_data) => {
            // println!("OutReportError");
            let event = EventOut::QueryError { error, user_data };
            (None, Some(ProtocolsHandlerEvent::Custom(event)), false)
        }
        OutClosing(mut stream) => {
            // println!("OutClosing");
            match Sink::poll_close(Pin::new(&mut stream), cx) {
                Poll::Ready(Ok(())) => (None, None, false),
                Poll::Pending => (Some(OutClosing(stream)), None, false),
                Poll::Ready(Err(_)) => (None, None, false),
            }
        }
        InWaitingMessage(id, mut substream) => {
            // println!("InWaitingMessage");
            match Stream::poll_next(Pin::new(&mut substream), cx) {
                Poll::Ready(Some(Ok(msg))) => {
                    if let Some(Ok(ev)) = process_request(msg, id) {
                        (
                            Some(InWaitingUser(id, substream)),
                            Some(ProtocolsHandlerEvent::Custom(ev)),
                            false,
                        )
                    } else {
                        (Some(InClosing(substream)), None, true)
                    }
                }
                Poll::Pending => (Some(InWaitingMessage(id, substream)), None, false),
                Poll::Ready(None) => {
                    // eprintln!("Inbound substream: EOF");
                    (None, None, false)
                }
                Poll::Ready(Some(Err(e))) => {
                    eprintln!("Inbound substream error: {:?}", e);
                    (None, None, false)
                }
            }
        }
        InWaitingUser(id, substream) => {
            // println!("InWaitingUser");
            (Some(InWaitingUser(id, substream)), None, false)
        }
        InPendingSend(id, mut substream, msg) => {
            // println!("InPendingSend");
            match Sink::poll_ready(Pin::new(&mut substream), cx) {
                Poll::Ready(Ok(())) => match Sink::start_send(Pin::new(&mut substream), msg) {
                    Ok(()) => (Some(InPendingFlush(id, substream)), None, true),
                    Err(_) => (None, None, false),
                },
                Poll::Pending => (Some(InPendingSend(id, substream, msg)), None, false),
                Poll::Ready(Err(_)) => (None, None, false),
            }
        }
        InPendingFlush(id, mut substream) => {
            // println!("InPendingFlush");
            match Sink::poll_flush(Pin::new(&mut substream), cx) {
                Poll::Ready(Ok(())) => (Some(InWaitingMessage(id, substream)), None, true),
                Poll::Pending => (Some(InPendingFlush(id, substream)), None, false),
                Poll::Ready(Err(_)) => (None, None, false),
            }
        }
        InClosing(mut stream) => {
            // println!("InClosing");
            match Sink::poll_close(Pin::new(&mut stream), cx) {
                Poll::Ready(Ok(())) => (None, None, false),
                Poll::Pending => (Some(InClosing(stream)), None, false),
                Poll::Ready(Err(_)) => (None, None, false),
            }
        }
    }
}
