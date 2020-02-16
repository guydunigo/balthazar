//! Provides [`Balthandler`], a [`ProtocolsHandler`] for use in [`libp2p`].
//!
//! This module is greatly inspired from [`libp2p::kad::handler::KademliaHandler`].
//!
//! ## Procedure when adding events or new kinds of messages
//!
//! To handle new messages:
//! 1. add new events in [`EventIn`] and [`EventOut`],
//! 2. for each new [`EventIn`], extend the `inject_event` function,
//! 3. for each new request message which should be forwarded to the behviour, extend
//!    the `process_request` function to create the corresponding [`EventOut`],
//! 4. for each new answer message which should be forwarded to the behviour, extend
//!    the `process_answer` function to create the corresponding [`EventOut`].
use futures::io::{AsyncRead, AsyncWrite};
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
    task::{Context, Poll},
    time::{Duration, Instant},
};

use misc::{NodeType, TaskStatus};
use proto::{protobuf::ProtoBufProtocol, worker};

mod handler_misc;
use handler_misc::*;

/// Default time to keep alive time to determine how long should the connection be
/// kept with the peer.
pub const DEFAULT_KEEP_ALIVE_DURATION_SECS: u64 = 10;

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
    type InEvent = EventIn<TUserData>;
    type OutEvent = EventOut<TUserData>;
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
            // Finds the corresponding user request to send the answer with the correct substream:
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
            EventIn::NodeTypeRequest { user_data } => {
                let msg = worker::NodeTypeRequest {}.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::NodeTypeAnswer {
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
            EventIn::ManagerRequest { user_data } => {
                let msg = worker::ManagerRequest {}.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::ManagerAnswer {
                accepted,
                request_id,
            } => {
                let msg = worker::ManagerAnswer { accepted }.into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            EventIn::NotMine { request_id } => {
                let msg = worker::NotMine {}.into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            EventIn::ManagerBye { user_data } => {
                let msg = worker::ManagerBye {}.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::ManagerByeAnswer { request_id } => {
                let msg = worker::ManagerBye {}.into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            EventIn::ExecuteTask {
                job_addr,
                argument,
                user_data,
            } => {
                let msg = worker::ExecuteTask { job_addr, argument }.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::TaskResult { result, request_id } => {
                let msg = worker::TaskResult { result }.into();
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

// ---------------------------------------------------------------

/// Events coming from the [`BalthBehaviour`](`super::BalthBehaviour`) into the [`Balthandler`]
/// to be sent to the peer for instance.
///
/// For each event for message added, add either of these fields to identify different events:
/// - `user_data`: for requests coming from us (i.e. through the
/// [`BalthBehaviour`](`super::BalthBehaviour`)),
/// - `request_id`: for answers at peer's requests coming.
#[derive(Debug)]
pub enum EventIn<TUserData> {
    NodeTypeRequest {
        user_data: TUserData,
    },
    NodeTypeAnswer {
        node_type: NodeType,
        request_id: RequestId,
    },
    NotMine {
        request_id: RequestId,
    },
    Ack {
        request_id: RequestId,
    },
    ManagerRequest {
        cpu_count: u64,
        memory: u64,
        network_speed: u64,
        worker_price: u64,
        user_data: TUserData,
    },
    ManagerAnswer {
        accepted: bool,
        request_id: RequestId,
    },
    ManagerBye {
        user_data: TUserData,
    },
    ManagerPing {
        user_data: TUserData,
    },
    ManagerPong {
        request_id: RequestId,
    },
    TasksExecute {
        job_id: Vec<u8>,
        user_data: TUserData,
    },
    TasksPing {
        task_ids: Vec<Vec<u8>>,
        user_data: TUserData,
    },
    TasksPong {
        statuses: Vec<(Vec<u8>, TaskStatus)>,
        request_id: RequestId,
    },
    TaskStatus {
        task_id: Vec<u8>,
        status: TaskStatus,
        user_data: TUserData,
    },
}

/// Events coming out of [`Balthandler`]. It can be forwarding a message coming
/// from a peer to the [`BalthBehaviour`](`super::BalthBehaviour`) for example.
///
/// For each event for message added, add either of these fields to identify different events:
/// - `user_data`: for answers to our requests from the peer,
/// - `request_id`: for requests coming from the peer (i.e. through the [`BalthBehaviour`](`super::BalthBehaviour`)).
#[derive(Debug)]
pub enum EventOut<TUserData> {
    NodeTypeAnswer {
        node_type: NodeType,
        user_data: TUserData,
    },
    NodeTypeRequest {
        request_id: RequestId,
    },
    NotMine {
        user_data: TUserData,
    },
    ManagerRequest {
        cpu_count: u64,
        memory: u64,
        network_speed: u64,
        worker_price: u64,
        request_id: RequestId,
    },
    ManagerAnswer {
        accepted: bool,
        user_data: TUserData,
    },
    ManagerBye {
        request_id: RequestId,
    },
    ManagerPing {
        request_id: RequestId,
    },
    // TODO: useful to propagate since it doesn't create any actions ?
    ManagerPong {
        user_data: TUserData,
    },
    TasksExecute {
        job_id: Vec<u8>,
        request_id: RequestId,
    },
    TasksPing {
        task_ids: Vec<Vec<u8>>,
        request_id: RequestId,
    },
    TasksPong {
        statuses: Vec<(Vec<u8>, TaskStatus)>,
        user_data: TUserData,
    },
    TaskStatus {
        task_id: Vec<u8>,
        status: TaskStatus,
        request_id: RequestId,
    },
    QueryError {
        error: BalthandlerQueryErr,
        user_data: TUserData,
    },
}

/// Processes a message that's expected to be a request from a remote.
fn process_request<TUserData>(
    event: WorkerMsgWrapper,
    connec_unique_id: UniqueConnecId,
) -> Option<Result<EventOut<TUserData>, io::Error>> {
    // eprintln!("process_request {:?}", event);
    if let Some(msg) = event.msg {
        match msg {
            WorkerMsg::NodeTypeRequest(worker::NodeTypeRequest {}) => {
                Some(Ok(EventOut::NodeTypeRequest {
                    request_id: RequestId::new(connec_unique_id),
                }))
            }
            WorkerMsg::ExecuteTask(worker::ExecuteTask { job_addr, argument }) => {
                Some(Ok(EventOut::ExecuteTask {
                    job_addr,
                    argument,
                    request_id: RequestId::new(connec_unique_id),
                }))
            }
            WorkerMsg::ManagerRequest(worker::ManagerRequest {}) => {
                Some(Ok(EventOut::ManagerRequest {
                    request_id: RequestId::new(connec_unique_id),
                }))
            }
            WorkerMsg::ManagerBye(worker::ManagerBye {}) => Some(Ok(EventOut::ManagerBye {
                request_id: RequestId::new(connec_unique_id),
            })),
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
) -> Option<EventOut<TUserData>> {
    // eprintln!("process_answer {:?}", event);
    if let Some(msg) = event.msg {
        match msg /*event.msg.expect("empty protobuf oneof")*/ {
            WorkerMsg::NodeTypeAnswer(worker::NodeTypeAnswer { node_type }) => {
                let node_type = worker::NodeType::from_i32(node_type)
                    .unwrap_or_else(|| panic!(
                        "Unexpected i32 value in protobuf enum NodeTypeAnswer::node_type: `{}`",
                        node_type
                    ))
                    .into();
                Some(EventOut::NodeTypeAnswer {
                    node_type,
                    user_data,
                })
            }
            WorkerMsg::TaskResult(worker::TaskResult { result }) => {
                Some(EventOut::TaskResult {
                    result,
                    user_data,
                })
            }
            WorkerMsg::ManagerAnswer(worker::ManagerAnswer { accepted }) => {
                Some(EventOut::ManagerAnswer { accepted, user_data })
            }
            WorkerMsg::NotMine(worker::NotMine {}) => {
                Some(EventOut::NotMine { user_data })
            }
            WorkerMsg::ManagerByeAnswer(_) => {
                // We don't need to process such answer, as it was just a notice message.
                None
            }
            _ => None,
        }
    } else {
        None
    }
}
