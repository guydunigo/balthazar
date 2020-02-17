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
    collections::HashMap,
    fmt, io,
    iter::FromIterator,
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
            EventIn::NodeTypeRequest {
                node_type,
                user_data,
            } => {
                let msg = worker::NodeTypeRequest {
                    node_type: node_type.into(),
                }
                .into();
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
            EventIn::NotMine { request_id } => {
                let msg = worker::NotMine {}.into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            EventIn::Ack { request_id } => {
                let msg = worker::Ack {}.into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            EventIn::ManagerRequest {
                cpu_count,
                memory,
                network_speed,
                worker_price,
                user_data,
            } => {
                let msg = worker::ManagerRequest {
                    cpu_count,
                    memory,
                    network_speed,
                    worker_price,
                }
                .into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::ManagerAnswer {
                accepted,
                request_id,
            } => {
                let msg = worker::ManagerAnswer { accepted }.into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            EventIn::ManagerBye { user_data } => {
                let msg = worker::ManagerBye {}.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::ManagerPing { user_data } => {
                let msg = worker::ManagerPing {}.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::ManagerPong { request_id } => {
                let msg = worker::ManagerPing {}.into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            EventIn::TasksExecute { tasks, user_data } => {
                let msg = worker::TasksExecute {
                    tasks: tasks.drain().map(|(_, t)| t).collect(),
                }
                .into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::TasksPing {
                task_ids,
                user_data,
            } => {
                let msg = worker::TasksPing { task_ids }.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::TasksPong {
                statuses,
                request_id,
            } => {
                let msg = worker::TasksPong {
                    statuses: statuses
                        .drain()
                        .map(|(i, s)| worker::TaskStatus {
                            task_id: i,
                            status_data: s.into(),
                        })
                        .collect(),
                }
                .into();
                inject_answer_event_to_peer_request(&mut self.substreams, request_id, msg)
            }
            EventIn::TasksAbord {
                task_ids,
                user_data,
            } => {
                let msg = worker::TasksAbord { task_ids }.into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
            }
            EventIn::TaskStatus {
                task_id,
                status,
                user_data,
            } => {
                let msg = worker::TaskStatus {
                    task_id,
                    status_data: status.into(),
                }
                .into();
                inject_new_request_event(&mut self.substreams, user_data, msg)
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
        node_type: NodeType,
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
        tasks: HashMap<Vec<u8>, worker::TaskExecute>,
        user_data: TUserData,
    },
    TasksPing {
        task_ids: Vec<Vec<u8>>,
        user_data: TUserData,
    },
    TasksPong {
        statuses: HashMap<Vec<u8>, TaskStatus>,
        request_id: RequestId,
    },
    TasksAbord {
        task_ids: Vec<Vec<u8>>,
        user_data: TUserData,
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
    NodeTypeRequest {
        node_type: NodeType,
        request_id: RequestId,
    },
    NodeTypeAnswer {
        node_type: NodeType,
        user_data: TUserData,
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
        tasks: HashMap<Vec<u8>, worker::TaskExecute>,
        request_id: RequestId,
    },
    TasksPing {
        task_ids: Vec<Vec<u8>>,
        request_id: RequestId,
    },
    TasksPong {
        statuses: HashMap<Vec<u8>, TaskStatus>,
        user_data: TUserData,
    },
    TasksAbord {
        task_ids: Vec<Vec<u8>>,
        request_id: RequestId,
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
/// Basically transforms request messages into handler's [`EventOut`].
fn process_request<TUserData>(
    event: WorkerMsgWrapper,
    connec_unique_id: UniqueConnecId,
) -> Option<Result<EventOut<TUserData>, io::Error>> {
    // eprintln!("process_request {:?}", event);
    if let Some(msg) = event.msg {
        match msg {
            WorkerMsg::NodeTypeRequest(worker::NodeTypeRequest { node_type }) => {
                let node_type = worker::NodeType::from_i32(node_type)
                    .unwrap_or_else(|| {
                        eprintln!(
                        "E -- Unexpected i32 value in protobuf enum NodeTypeAnswer::node_type: `{}`, using default: `{:?}`.",
                        node_type,
                        worker::NodeType::default()
                    );
                        worker::NodeType::default()
                    });
                Some(Ok(EventOut::NodeTypeRequest {
                    node_type,
                    request_id: RequestId::new(connec_unique_id),
                }))
            }
            WorkerMsg::ManagerRequest(worker::ManagerRequest {
                cpu_count,
                memory,
                network_speed,
                worker_price,
            }) => Some(Ok(EventOut::ManagerRequest {
                cpu_count,
                memory,
                network_speed,
                worker_price,
                request_id: RequestId::new(connec_unique_id),
            })),
            WorkerMsg::ManagerBye(worker::ManagerBye {}) => Some(Ok(EventOut::ManagerBye {
                request_id: RequestId::new(connec_unique_id),
            })),
            WorkerMsg::ManagerPing(worker::ManagerPing {}) => Some(Ok(EventOut::ManagerPing {
                request_id: RequestId::new(connec_unique_id),
            })),
            WorkerMsg::TasksExecute(worker::TasksExecute { tasks }) => {
                Some(Ok(EventOut::TasksExecute {
                    tasks: HashMap::from_iter(tasks.drain(..).map(|t| (t.job_id, t))),
                    request_id: RequestId::new(connec_unique_id),
                }))
            }
            WorkerMsg::TasksPing(worker::TasksPing { task_ids }) => Some(Ok(EventOut::TasksPing {
                task_ids,
                request_id: RequestId::new(connec_unique_id),
            })),
            WorkerMsg::TasksAbord(worker::TasksAbord { task_ids }) => {
                Some(Ok(EventOut::TasksAbord {
                    task_ids,
                    request_id: RequestId::new(connec_unique_id),
                }))
            }
            WorkerMsg::TaskStatus(worker::TaskStatus {
                task_id,
                status_data,
            }) => Some(Ok(EventOut::TaskStatus {
                task_id,
                status: status_data.into(),
                request_id: RequestId::new(connec_unique_id),
            })),
            _ => None,
        }
    } else {
        None
    }
}

/// Process a message that's supposed to be an answer to one of our requests.
/// Basically transforms answer messages into handler's [`EventOut`].
fn process_answer<TUserData>(
    event: WorkerMsgWrapper,
    user_data: TUserData,
) -> Option<EventOut<TUserData>> {
    // eprintln!("process_answer {:?}", event);
    if let Some(msg) = event.msg {
        match msg /*event.msg.expect("empty protobuf oneof")*/ {
            WorkerMsg::NodeTypeAnswer(worker::NodeTypeAnswer { node_type }) => {
                let node_type = worker::NodeType::from_i32(node_type)
                    .unwrap_or_else(|| {
                        eprintln!(
                        "E -- Unexpected i32 value in protobuf enum NodeTypeAnswer::node_type: `{}`, using default: `{:?}`.",
                        node_type,
                        worker::NodeType::default()
                    );
                        worker::NodeType::default()
                    });
                Some(EventOut::NodeTypeAnswer {
                    node_type,
                    user_data,
                })
            }
            WorkerMsg::NotMine(worker::NotMine {}) => {
                Some(EventOut::NotMine { user_data })
            }
            WorkerMsg::Ack(worker::Ack {}) => None,
            WorkerMsg::ManagerAnswer(worker::ManagerAnswer { accepted }) => {
                Some(EventOut::ManagerAnswer { accepted, user_data })
            }
            WorkerMsg::ManagerPong(worker::ManagerPong { }) => {
                Some(EventOut::ManagerPong { user_data })
            }
            WorkerMsg::TasksPong(worker::TasksPong { statuses }) => {
                Some(EventOut::TasksPong {
                    statuses: HashMap::from_iter(statuses.iter().map(|s| (s.task_id,s.status_data.into()))),
                    user_data,
                })
            }
            _ => None,
        }
    } else {
        None
    }
}
