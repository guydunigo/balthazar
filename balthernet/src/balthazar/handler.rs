//!  This module is greatly inspired from [`libp2p::kad::handler::KademliaHandler`].
//!
//!  It provides [`Balthandler`], a [`ProtocolsHandler`] for use in [`libp2p`].
//!
//!  TODO: add procedure for new messages.
use futures::{
    io::{AsyncRead, AsyncWrite},
    sink::Sink,
    Stream,
};
use libp2p::{
    core::{InboundUpgrade, OutboundUpgrade},
    swarm::{
        KeepAlive, ProtocolsHandler, ProtocolsHandlerEvent, ProtocolsHandlerUpgrErr,
        SubstreamProtocol,
    },
};
use proto::worker::{WorkerMsg, WorkerMsgWrapper};
use std::{
    error, fmt, io,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use misc::NodeType;
use proto::{
    protobuf::{ProtoBufProtocol, ProtoBufProtocolSink},
    worker,
};

/// Default time to keep alive time to determine how long should the connection be
/// kept with the peer.
pub const DEFAULT_KEEP_ALIVE_DURATION_SECS: u64 = 10;

// TODO: reference to the NetworkBehaviour
/// Events coming from the NetworkBehaviour into the [`Balthandler`] to be sent to the peer for instance.
#[derive(Debug)]
pub enum BalthandlerEventIn<TUserData> {
    ManagerRequest {
        user_data: TUserData,
    },
    ManagerAnswer {
        accepted: bool,
        request_id: MyRequestId,
    },
    NodeTypeRequest {
        user_data: TUserData,
    },
    NodeTypeAnswer {
        node_type: NodeType<()>,
        request_id: MyRequestId,
    },
}

// TODO: reference to the NetworkBehaviour
/// Events coming out of [`Balthandler`]. It can be forwarding a message coming
/// from a peer to the NetworkBehaviour for example.
#[derive(Debug)] // todo: copy?
pub enum BalthandlerEventOut<TUserData> {
    NodeTypeAnswer {
        node_type: NodeType<()>,
        user_data: TUserData,
    },
    NodeTypeRequest {
        request_id: MyRequestId,
    },
    ManagerRequest {
        request_id: MyRequestId,
    },
    ManagerAnswer {
        answer: bool,
        user_data: TUserData,
    },
    QueryError {
        error: BalthandlerQueryErr,
        user_data: TUserData,
    },
}

/// Unique identifier for a request. Must be passed back in order to answer a request from
/// the remote.
///
/// We don't implement `Clone` on purpose, in order to prevent users from answering the same
/// request twice.
#[derive(Debug, PartialEq, Eq)]
pub struct MyRequestId {
    /// Unique identifier for an incoming connection.
    connec_unique_id: UniqueConnecId,
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

/// Unique identifier for a connection.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct UniqueConnecId(u64);

/// State of an active substream, opened either by us or by the remote.
enum SubstreamState<TMessage, TUserData> {
    /// We haven't started opening the outgoing substream yet.
    /// Contains the request we want to send, and the user data if we expect an answer.
    OutPendingOpen(TMessage, Option<TUserData>),
    /// Waiting to send a message to the remote.
    OutPendingSend(ProtoBufProtocolSink<TMessage>, TMessage, Option<TUserData>),
    /// Waiting to flush the substream so that the data arrives to the remote.
    OutPendingFlush(ProtoBufProtocolSink<TMessage>, Option<TUserData>),
    /// Waiting for an answer back from the remote.
    // TODO: add timeout
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

/// This structure implements the [`ProtocolsHandler`] trait to handle a connection with
/// another peer.
pub struct Balthandler<TSubstream, TUserData>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin,
{
    substreams: Vec<SubstreamState<WorkerMsgWrapper, TUserData>>,
    proto: ProtoBufProtocol<WorkerMsgWrapper>,
    keep_alive: KeepAlive,
    next_connec_unique_id: UniqueConnecId,
    _marker: std::marker::PhantomData<TSubstream>,
}

impl<TSubstream, TUserData> Balthandler<TSubstream, TUserData>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a [`KeepAlive`] object with a default time to keep alive time to determine
    /// how long should the connection be kept with the peer.
    ///
    /// See [`DEFAULT_KEEP_ALIVE_DURATION_SECS`].
    fn default_keep_alive() -> KeepAlive {
        KeepAlive::Until(Instant::now() + Duration::from_secs(DEFAULT_KEEP_ALIVE_DURATION_SECS))
    }

    fn next_connec_unique_id(&mut self) -> UniqueConnecId {
        let old = self.next_connec_unique_id;
        self.next_connec_unique_id = UniqueConnecId(old.0 + 1);
        old
    }
}

impl<TSubstream, TUserData> Default for Balthandler<TSubstream, TUserData>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin,
{
    fn default() -> Self {
        Balthandler {
            substreams: Vec::new(),
            proto: worker::new_worker_protocol(),
            keep_alive: Self::default_keep_alive(),
            next_connec_unique_id: UniqueConnecId(0),
            _marker: Default::default(),
        }
    }
}

impl<TSubstream, TUserData> ProtocolsHandler for Balthandler<TSubstream, TUserData>
where
    TSubstream: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    TUserData: fmt::Debug, // + Clone,
{
    type InEvent = BalthandlerEventIn<TUserData>;
    type OutEvent = BalthandlerEventOut<TUserData>;
    type Error = io::Error;
    type Substream = TSubstream;
    type InboundProtocol = ProtoBufProtocol<WorkerMsgWrapper>;
    type OutboundProtocol = ProtoBufProtocol<WorkerMsgWrapper>;
    type OutboundOpenInfo = (WorkerMsgWrapper, Option<TUserData>);

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol> {
        // eprintln!("New listen protocol");
        SubstreamProtocol::new(worker::new_worker_protocol())
    }

    fn inject_fully_negotiated_inbound(
        &mut self,
        sink: <Self::InboundProtocol as InboundUpgrade<TSubstream>>::Output,
    ) {
        // eprintln!("New Inbound Frame received after successful upgrade.");
        let next_connec_unique_id = self.next_connec_unique_id();
        self.substreams.push(SubstreamState::InWaitingMessage(
            next_connec_unique_id,
            sink,
        ));
    }

    fn inject_fully_negotiated_outbound(
        &mut self,
        sink: <Self::OutboundProtocol as OutboundUpgrade<TSubstream>>::Output,
        (msg, user_data): Self::OutboundOpenInfo,
    ) {
        // eprintln!("New Outbout Frame received after successful upgrade.");
        self.substreams
            .push(SubstreamState::OutPendingSend(sink, msg, user_data));
    }

    fn inject_event(&mut self, event: Self::InEvent) {
        eprintln!("Event injected in Handler from Behaviour: {:?}", event);
        match event {
            BalthandlerEventIn::ManagerAnswer {
                accepted,
                request_id,
            } => {
                let pos = self.substreams.iter().position(|state| match state {
                    SubstreamState::InWaitingUser(ref conn_id, _) => {
                        conn_id == &request_id.connec_unique_id
                    }
                    _ => false,
                });

                if let Some(pos) = pos {
                    let (conn_id, substream) = match self.substreams.remove(pos) {
                        SubstreamState::InWaitingUser(conn_id, substream) => (conn_id, substream),
                        _ => unreachable!(),
                    };

                    let msg = worker::ManagerAnswer { accepted }.into();
                    let evt = SubstreamState::InPendingSend(conn_id, substream, msg);
                    self.substreams.push(evt);
                }
            }
            BalthandlerEventIn::ManagerRequest { user_data } => {
                let msg = worker::ManagerRequest {}.into();
                let evt = SubstreamState::OutPendingOpen(msg, Some(user_data));
                self.substreams.push(evt);
            }
            BalthandlerEventIn::NodeTypeRequest { user_data } => {
                let msg = worker::NodeTypeRequest {}.into();
                let evt = SubstreamState::OutPendingOpen(msg, Some(user_data));
                self.substreams.push(evt);
            }
            BalthandlerEventIn::NodeTypeAnswer {
                node_type,
                request_id,
            } => {
                let pos = self.substreams.iter().position(|state| match state {
                    SubstreamState::InWaitingUser(ref conn_id, _) => {
                        conn_id == &request_id.connec_unique_id
                    }
                    _ => false,
                });

                if let Some(pos) = pos {
                    let (conn_id, substream) = match self.substreams.remove(pos) {
                        SubstreamState::InWaitingUser(conn_id, substream) => (conn_id, substream),
                        _ => unreachable!(),
                    };

                    let mut msg = worker::NodeTypeAnswer::default();
                    msg.set_node_type(node_type.into());

                    let evt = SubstreamState::InPendingSend(conn_id, substream, msg.into());
                    self.substreams.push(evt);
                }
            }
            BalthandlerEventIn::Dummy => (),
        }
    }

    fn inject_dial_upgrade_error(
        &mut self,
        (_, user_data): Self::OutboundOpenInfo,
        error: ProtocolsHandlerUpgrErr<
            <Self::OutboundProtocol as OutboundUpgrade<TSubstream>>::Error,
        >,
    ) {
        eprintln!("Dial upgrade error: {:?}", error);
        if let Some(user_data) = user_data {
            self.substreams
                .push(SubstreamState::OutReportError(error.into(), user_data));
        }
    }

    fn connection_keep_alive(&self) -> KeepAlive {
        let decision = self.keep_alive;
        // eprintln!("Should the connection be kept alive ? {:?}", decision);
        decision
    }

    fn poll(
        &mut self,
        cx: &mut Context,
    ) -> Poll<
        ProtocolsHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::OutEvent,
            Self::Error,
        >,
    > {
        if self.substreams.is_empty() {
            return Poll::Pending;
        }

        for n in (0..self.substreams.len()).rev() {
            let mut substream = self.substreams.swap_remove(n);

            loop {
                match advance_substream::<TSubstream, TUserData>(substream, self.proto.clone(), cx)
                {
                    (Some(new_state), Some(evt), _) => {
                        // eprintln!("A : {}", self.substreams.len());
                        self.substreams.push(new_state);
                        return Poll::Ready(evt);
                    }
                    (None, Some(evt), _) => {
                        // eprintln!("B : {}", self.substreams.len());
                        if self.substreams.is_empty() {
                            self.keep_alive = Self::default_keep_alive();
                        }
                        return Poll::Ready(evt);
                    }
                    (Some(new_state), None, false) => {
                        // eprintln!("C : {}", self.substreams.len());
                        self.substreams.push(new_state);
                        break;
                    }
                    (Some(new_state), None, true) => {
                        // eprintln!("D : {}", self.substreams.len());
                        substream = new_state;
                        continue;
                    }
                    (None, None, _) => break,
                }
            }
        }

        Poll::Pending
    }
}

/// Advances one substream.
///
/// Returns the new state for that substream, an event to generate, and whether the substream
/// should be polled again.
fn advance_substream<TSubstream, TUserData>(
    state: SubstreamState<WorkerMsgWrapper, TUserData>,
    upgrade: ProtoBufProtocol<WorkerMsgWrapper>,
    cx: &mut Context,
) -> (
    Option<SubstreamState<WorkerMsgWrapper, TUserData>>,
    Option<
        ProtocolsHandlerEvent<
            ProtoBufProtocol<WorkerMsgWrapper>,
            (WorkerMsgWrapper, Option<TUserData>),
            BalthandlerEventOut<TUserData>,
            io::Error,
        >,
    >,
    bool,
)
where
    TSubstream: AsyncRead + AsyncWrite + Unpin,
{
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
                            Some(ProtocolsHandlerEvent::Custom(
                                BalthandlerEventOut::QueryError {
                                    error: BalthandlerQueryErr::Io(error),
                                    user_data,
                                },
                            ))
                        } else {
                            None
                        };
                        (None, event, false)
                    }
                },
                Poll::Pending => (Some(OutPendingSend(substream, msg, user_data)), None, false),
                Poll::Ready(Err(error)) => {
                    let event = if let Some(user_data) = user_data {
                        Some(ProtocolsHandlerEvent::Custom(
                            BalthandlerEventOut::QueryError {
                                error: BalthandlerQueryErr::Io(error),
                                user_data,
                            },
                        ))
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
                        Some(ProtocolsHandlerEvent::Custom(
                            BalthandlerEventOut::QueryError {
                                error: BalthandlerQueryErr::Io(error),
                                user_data,
                            },
                        ))
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
                    let event = BalthandlerEventOut::QueryError {
                        error: error.into(),
                        user_data,
                    };
                    (None, Some(ProtocolsHandlerEvent::Custom(event)), false)
                }
                Poll::Ready(None) => {
                    let event = BalthandlerEventOut::QueryError {
                        error: BalthandlerQueryErr::Io(io::ErrorKind::UnexpectedEof.into()),
                        user_data,
                    };
                    (None, Some(ProtocolsHandlerEvent::Custom(event)), false)
                }
            }
        }
        OutReportError(error, user_data) => {
            // println!("OutReportError");
            let event = BalthandlerEventOut::QueryError { error, user_data };
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
                    eprintln!("Inbound substream: EOF");
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

/// Processes a message that's expected to be a request from a remote.
fn process_request<TUserData>(
    event: WorkerMsgWrapper,
    connec_unique_id: UniqueConnecId,
) -> Option<Result<BalthandlerEventOut<TUserData>, io::Error>> {
    eprintln!("process_request {:?}", event);
    if let Some(msg) = event.msg {
        match msg /*event.msg.expect("empty protobuf oneof")*/ {
        WorkerMsg::NodeTypeRequest(worker::NodeTypeRequest {}) => {
            Some(Ok(BalthandlerEventOut::NodeTypeRequest {
                request_id: MyRequestId { connec_unique_id },
            }))
        }
        _ => None,
    }
    } else {
        None
    }
}

/// Process a message that's supposed to be an answer to one of our requests.
fn process_answer<TUserData>(
    event: WorkerMsgWrapper,
    user_data: TUserData,
) -> Option<BalthandlerEventOut<TUserData>> {
    eprintln!("process_answer {:?}", event);
    if let Some(msg) = event.msg {
        match msg /*event.msg.expect("empty protobuf oneof")*/ {
        WorkerMsg::NodeTypeAnswer(worker::NodeTypeAnswer { node_type }) => {
            let node_type = worker::NodeType::from_i32(node_type)
                .expect(&format!(
                    "Unexpected i32 value in protobuf enum NodeTypeAnswer::node_type: `{}`",
                    node_type
                ))
                .into();
            Some(BalthandlerEventOut::NodeTypeAnswer {
                node_type,
                user_data,
            })
        }
        _ => None,
    }
    } else {
        None
    }
}
