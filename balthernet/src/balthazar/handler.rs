//! This module is greatly inspired from [`libp2p::kad::handler::KademliaHandler`].
//!
//! It provides [`Balthandler`], a [`ProtocolsHandler`] for use in [`libp2p`].
//!
//! TODO: add procedure for new messages.
//! To handle new messages:
//! 1. add new events in [`BalthandlerEventIn`] and [`BalthandlerEventOut`],
//! 2. for each new [`BalthandlerEventIn`], extend [`Balthandler::inject_event`],
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
    fmt, io,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use misc::NodeType;
use proto::{protobuf::ProtoBufProtocol, worker};

mod handler_misc;
use handler_misc::*;

/// Default time to keep alive time to determine how long should the connection be
/// kept with the peer.
pub const DEFAULT_KEEP_ALIVE_DURATION_SECS: u64 = 10;

// TODO: reference to the NetworkBehaviour
/// Events coming from the NetworkBehaviour into the [`Balthandler`] to be sent to the peer for instance.
///
/// For each event for message added, add either of these fields to identify different events:
/// - `user_data`: for requests coming from us (i.e. through the NetworkBehaviour),
/// - `request_id`: for answers at peer's requests coming.
#[derive(Debug)]
pub enum BalthandlerEventIn<TUserData> {
    ManagerRequest {
        user_data: TUserData,
    },
    ManagerAnswer {
        accepted: bool,
        request_id: RequestId,
    },
    NodeTypeRequest {
        user_data: TUserData,
    },
    NodeTypeAnswer {
        node_type: NodeType<()>,
        request_id: RequestId,
    },
}

// TODO: reference to the NetworkBehaviour
/// Events coming out of [`Balthandler`]. It can be forwarding a message coming
/// from a peer to the NetworkBehaviour for example.
///
/// For each event for message added, add either of these fields to identify different events:
/// - `user_data`: for answers to our requests from the peer,
/// - `request_id`: for requests coming from the peer (i.e. through the NetworkBehaviour).
#[derive(Debug)]
pub enum BalthandlerEventOut<TUserData> {
    NodeTypeAnswer {
        node_type: NodeType<()>,
        user_data: TUserData,
    },
    NodeTypeRequest {
        request_id: RequestId,
    },
    ManagerRequest {
        request_id: RequestId,
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

    /// Gets a new unique identifier for a message request from the peer and generates a new one.
    fn next_connec_unique_id(&mut self) -> UniqueConnecId {
        let old = self.next_connec_unique_id;
        self.next_connec_unique_id = self.next_connec_unique_id.inc();
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
            next_connec_unique_id: Default::default(),
            _marker: Default::default(),
        }
    }
}

impl<TSubstream, TUserData> ProtocolsHandler for Balthandler<TSubstream, TUserData>
where
    TSubstream: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    TUserData: fmt::Debug,
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
        fn inject_new_request_event<TUserData>(
            substreams: &mut Vec<SubstreamState<WorkerMsgWrapper, TUserData>>,
            user_data: TUserData,
            msg_to_send: worker::WorkerMsgWrapper,
        ) {
            let evt = SubstreamState::OutPendingOpen(msg_to_send, Some(user_data));
            substreams.push(evt);
        }

        fn inject_answer_event_to_peer_request<TUserData>(
            substreams: &mut Vec<SubstreamState<WorkerMsgWrapper, TUserData>>,
            request_id: RequestId,
            msg_to_inject: worker::WorkerMsgWrapper,
        ) {
            let pos = substreams.iter().position(|state| match state {
                SubstreamState::InWaitingUser(ref conn_id, _) => {
                    conn_id == request_id.connec_unique_id()
                }
                _ => false,
            });

            if let Some(pos) = pos {
                let (conn_id, substream) = match substreams.remove(pos) {
                    SubstreamState::InWaitingUser(conn_id, substream) => (conn_id, substream),
                    _ => unreachable!(),
                };

                let evt = SubstreamState::InPendingSend(conn_id, substream, msg_to_inject);
                substreams.push(evt);
            }
        }

        // eprintln!("Event injected in Handler from Behaviour: {:?}", event);
        match event {
            BalthandlerEventIn::ManagerAnswer {
                accepted,
                request_id,
            } => {
                let msg = worker::ManagerAnswer { accepted }.into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            BalthandlerEventIn::ManagerRequest { user_data } => {
                let msg = worker::ManagerRequest {}.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            BalthandlerEventIn::NodeTypeRequest { user_data } => {
                let msg = worker::NodeTypeRequest {}.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            BalthandlerEventIn::NodeTypeAnswer {
                node_type,
                request_id,
            } => {
                let msg = {
                    let mut msg = worker::NodeTypeAnswer::default();
                    msg.set_node_type(node_type.into());
                    msg.into()
                };
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
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
        /*
            let decision = self.keep_alive;
            eprintln!("Should the connection be kept alive ? {:?}", decision);
            decision
        */
        self.keep_alive
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
                request_id: RequestId::new(connec_unique_id),
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
