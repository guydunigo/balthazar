//! Provides [`BalthBehaviour`], a [`NetworkBehaviour`] for use in [`libp2p`].
//!
//! This module is greatly inspired from [`libp2p::kad::Kademlia`].
//!
//! ## Procedure when adding events or new kinds of messages
//!
//! If there are new kinds of events that can be received, add them to [`InternalEvent`].
//!
//! Have a look at the [`handler`] module description to make the necessary updates.
//!
//! When extending [`HandlerOut`], update the `handler_event` function.
use futures::io::{AsyncRead, AsyncWrite};
use libp2p::{
    core::{nodes::ListenerId, ConnectedPoint},
    swarm::{
        protocols_handler::{IntoProtocolsHandler, ProtocolsHandler},
        NetworkBehaviour, NetworkBehaviourAction, PollParameters,
    },
    Multiaddr, PeerId,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    error,
    fmt::Debug,
    marker::PhantomData,
    rc::Rc,
    task::{Context, Poll},
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub mod handler;
use super::{ManagerConfig, WorkerConfig};
use handler::{Balthandler, EventIn as HandlerIn, EventOut as HandlerOut, RequestId};
use misc::{NodeType, NodeTypeContainer, WorkerSpecs};

const CHANNEL_SIZE: usize = 1024;

#[derive(Debug)]
struct ManagerData {
    config: ManagerConfig,
    workers: HashMap<PeerId, Rc<RefCell<Peer>>>,
}

#[derive(Debug)]
struct WorkerData {
    config: WorkerConfig,
    specs: WorkerSpecs,
    manager: Option<Rc<RefCell<Peer>>>,
}

type NodeTypeData = NodeTypeContainer<ManagerData, WorkerData>;

/// Event injected into [`BalthBehaviour`] from either [`Balthandler`] and from outside (other
/// [`NetworkBehaviour`]s, etc.
#[derive(Debug)]
enum InternalEvent<TUserData> {
    /// Event created when the [`Mdns`](`libp2p::mdns::Mdns`) discovers a new peer at given multiaddress.
    Mdns(PeerId, Multiaddr),
    /// Event originating from [`Balthandler`].
    Handler(PeerId, HandlerOut<TUserData>),
    /// Request a node type.
    /// TODO: delete and delegate to outside, or default here?
    AskNodeType(PeerId),
    /// Generates an event towards the Swarm.
    NetworkBehaviourAction(NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>),
}

/// TODO: doc
#[derive(Debug)]
pub enum EventIn {
    Ping,
    /// Sending a message to the peer (new request or answer to one from the exterior).
    Handler(PeerId, HandlerIn<QueryId>),
    /// Send ExecuteTask to peer.
    ExecuteTask {
        peer_id: PeerId,
        job_addr: Vec<u8>,
        argument: i32,
    },
}

/// Event returned by [`BalthBehaviour`] towards the Swarm when polled.
#[derive(Debug)]
pub enum EventOut {
    /// Node type discovered for a peer.
    PeerHasNewType(PeerId, NodeType),
    /// Peer has been connected to given endpoint.
    PeerConnected(PeerId, ConnectedPoint),
    /// Peer has been disconnected from given endpoint.
    PeerDisconnected(PeerId, ConnectedPoint),
    /// Answer to a [`EventIn::Ping`].
    Pong,
    /// Events created by [`Balthandler`] which are not handled directly in [`BalthBehaviour`]
    Handler(PeerId, HandlerOut<QueryId>),
    /// A new worker is now managed by us.
    WorkerNew(PeerId),
    /// When a worker stops being managed by us.
    WorkerBye(PeerId),
    /// When a worker sends us a message as if we were its manager.
    NotMyWorker(PeerId, HandlerOut<QueryId>),
    /// A manager has accepted acting as our manager, so we will receive orders from it now on.
    ManagerNew(PeerId),
    /// The manager we requested refused.
    ManagerRefused(PeerId),
    /// When the manager stops managing us.
    ManagerBye(PeerId),
    /// A manager accepted managing us, but it isn't authorized.
    ManagerUnauthorized(PeerId),
    /// A manager accepted managing us, but we already have one.
    ManagerAlreadyHasOne(PeerId),
    /// When a manager sends us a message as if we were one of its workers.
    NotMyManager(PeerId, HandlerOut<QueryId>),
    /// A message was received but we are the wrong NodeType to handle it.
    MsgForIncorrectNodeType {
        peer_id: PeerId,
        expected_type: NodeType,
        event: HandlerOut<QueryId>,
    },
    /// A message has been received from a peer which doesn't have the correct NodeType.
    MsgFromIncorrectNodeType {
        peer_id: PeerId,
        known_type: Option<NodeType>,
        expected_type: NodeType,
        event: HandlerOut<QueryId>,
    },
    /// Peer answered a different node type than what was before known.
    PeerGivesDifferentNodeType {
        peer_id: PeerId,
        previous: NodeType,
        new: NodeType,
    },
}

/// Type to identify our queries within [`Balthandler`] to link answers to queries.
pub type QueryId = usize;

/// Peer data as used by [`BalthBehaviour`].
#[derive(Clone, Debug)]
struct Peer {
    peer_id: PeerId,
    /// Known addresses
    addrs: HashSet<Multiaddr>,
    /// If there's an open connection, the connection infos
    endpoint: Option<ConnectedPoint>,
    /// Has it already been dialed at least once?
    /// TODO: use a date to recheck periodically ?
    dialed: bool,
    /// Defines the node type if it is known.
    node_type: Option<NodeTypeContainer<(), Option<WorkerSpecs>>>,
}

impl Peer {
    pub fn new(peer_id: PeerId) -> Self {
        Peer {
            peer_id,
            addrs: HashSet::new(),
            endpoint: None,
            dialed: false,
            node_type: None,
        }
    }

    /// Extracts known addresses into a Vec to be used more easily.
    pub fn addrs_as_vec(&self) -> Vec<Multiaddr> {
        self.addrs.iter().cloned().collect()
    }

    /// Transforms the inner `node_type` [`NodeTypeContainer`] data into a simpler [`NodeType`].
    pub fn node_type_into(&self) -> Option<NodeType> {
        self.node_type.map(|t| t.into())
    }
}

impl PartialEq<Self> for Peer {
    fn eq(&self, other: &Self) -> bool {
        self.peer_id == other.peer_id
    }
}

/// The [`NetworkBehaviour`] to manage the networking of the **Balthazar** node.
pub struct BalthBehaviour<TSubstream> {
    inbound_rx: Receiver<EventIn>,
    // TODO: should the node_type be kept here, what happens if it changes elsewhere?
    node_type_data: NodeTypeData,
    peers: HashMap<PeerId, Rc<RefCell<Peer>>>,
    events: VecDeque<InternalEvent<QueryId>>,
    next_query_unique_id: QueryId,
    _marker: PhantomData<TSubstream>,
}

impl<TSubstream> BalthBehaviour<TSubstream> {
    /// Creates a new [`BalthBehaviour`] and returns a [`Sender`] channel to communicate with it from
    /// the exterior of the Swarm.
    pub fn new(
        node_type_conf: NodeTypeContainer<ManagerConfig, (WorkerConfig, WorkerSpecs)>,
    ) -> (Self, Sender<EventIn>) {
        let (tx, inbound_rx) = channel(CHANNEL_SIZE);

        let node_type_data = match node_type_conf {
            NodeTypeContainer::Manager(config) => NodeTypeData::Manager(ManagerData {
                config,
                workers: HashMap::new(),
            }),
            NodeTypeContainer::Worker((config, specs)) => NodeTypeData::Worker(WorkerData {
                specs,
                config,
                manager: None,
            }),
        };

        (
            BalthBehaviour {
                inbound_rx,
                node_type_data,
                peers: HashMap::new(),
                events: VecDeque::new(),
                next_query_unique_id: 0,
                _marker: PhantomData,
            },
            tx,
        )
    }

    /// Gets a new unique identifier for a new message request generates a new one.
    pub fn next_query_unique_id(&mut self) -> QueryId {
        let old = self.next_query_unique_id;
        self.next_query_unique_id += 1;
        old
    }

    pub fn inject_mdns_event(&mut self, peer_id: PeerId, multiaddr: Multiaddr) {
        self.events
            .push_front(InternalEvent::Mdns(peer_id, multiaddr));
    }

    fn inject_handler_event(&mut self, peer_id: PeerId, handler_evt: HandlerOut<QueryId>) {
        self.events
            .push_front(InternalEvent::Handler(peer_id, handler_evt));
    }

    fn inject_generate_event(&mut self, evt: EventOut) {
        self.events
            .push_front(InternalEvent::NetworkBehaviourAction(
                NetworkBehaviourAction::GenerateEvent(evt),
            ));
    }

    fn inject_send_to_peer_event(&mut self, peer_id: PeerId, event: HandlerIn<QueryId>) {
        self.events
            .push_front(InternalEvent::NetworkBehaviourAction(
                NetworkBehaviourAction::SendEvent { peer_id, event },
            ));
    }

    fn inject_behaviour_action(
        &mut self,
        event: NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>,
    ) {
        self.events
            .push_front(InternalEvent::NetworkBehaviourAction(event));
    }

    fn get_peer_or_insert(&mut self, peer_id: &PeerId) -> Rc<RefCell<Peer>> {
        self.peers
            .entry(peer_id.clone())
            .or_insert_with(|| Rc::new(RefCell::new(Peer::new(peer_id.clone()))))
            .clone()
    }

    /*
    pub fn events(&self) -> &VecDeque<InternalEvent> {
        &self.events
    }

    pub fn events_mut(&mut self) -> &mut VecDeque<InternalEvent> {
        &mut self.events
    }
    */
}

impl<TSubstream> NetworkBehaviour for BalthBehaviour<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type ProtocolsHandler = Balthandler<TSubstream, QueryId>;
    type OutEvent = EventOut;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        // println!("New handler");
        Self::ProtocolsHandler::default()
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        // println!("Addresses of {:?} : {:?}", peer_id, addresses);
        self.peers
            .get(peer_id)
            .map(|p| p.borrow().addrs_as_vec())
            .unwrap_or_else(Vec::new)
    }

    fn inject_connected(&mut self, peer_id: PeerId, endpoint: ConnectedPoint) {
        let peer = self.get_peer_or_insert(&peer_id);
        let mut peer = peer.borrow_mut();

        if let Some(ref endpoint) = peer.endpoint {
            panic!(
                "Peer `{:?}` already has an endpoint `{:?}`.",
                peer_id, endpoint
            );
        } else {
            // If the node_type is unknown, plans to send a request:
            if peer.node_type.is_none() {
                self.events
                    .push_front(InternalEvent::AskNodeType(peer_id.clone()));
            }

            match endpoint.clone() {
                ConnectedPoint::Dialer { address } => {
                    peer.addrs.insert(address);
                }
                ConnectedPoint::Listener { send_back_addr, .. } => {
                    peer.addrs.insert(send_back_addr);
                }
            }

            peer.endpoint = Some(endpoint.clone());

            self.inject_generate_event(EventOut::PeerConnected(peer_id, endpoint));
        }
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId, endpoint: ConnectedPoint) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            if peer.borrow_mut().endpoint.take().is_some() {
                self.inject_generate_event(EventOut::PeerDisconnected(peer_id.clone(), endpoint));
            } else {
                panic!(
                    "Peer `{:?}` already doesn't have any endpoint `{:?}`.",
                    peer_id, endpoint
                );
            }
        } else {
            panic!(
                "Peer `{:?}` doesn't exist so can't remove endpoint `{:?}`.",
                peer_id, endpoint
            );
        }
    }

    fn inject_addr_reach_failure(
        &mut self,
        peer_id: Option<&PeerId>,
        addr: &Multiaddr,
        error: &dyn error::Error,
    ) {
        eprintln!(
            "ERR reach failure for : {:?} {:?} {:?}",
            peer_id, addr, error
        );
    }

    fn inject_dial_failure(&mut self, peer_id: &PeerId) {
        eprintln!("ERR dial failure for : {:?}", peer_id);
    }

    fn inject_listener_error(&mut self, id: ListenerId, err: &(dyn error::Error + 'static)) {
        eprintln!("ERR listener {:?} : {:?}", id, err);
    }

    fn inject_node_event(
        &mut self,
        peer_id: PeerId,
        event: <<Self::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::OutEvent,
    ) {
        self.inject_handler_event(peer_id, event);
    }

    // TODO: break this function into one or several external functions for clearer parsing ?
    // If yes, update the module doc.
    fn poll(
        &mut self,
        cx: &mut Context,
        _params: &mut impl PollParameters
) -> Poll<NetworkBehaviourAction<<<Self::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::InEvent, Self::OutEvent>>{
        // Go through the queued events and handle them:
        while let Some(internal_evt) = self.events.pop_back() {
            let answer = match internal_evt {
                InternalEvent::NetworkBehaviourAction(action) => Poll::Ready(action),
                InternalEvent::Mdns(peer_id, address) => {
                    let peer = self.get_peer_or_insert(&peer_id);
                    let mut peer = peer.borrow_mut();
                    peer.addrs.insert(address);

                    if !peer.dialed {
                        peer.dialed = true;
                        Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id })
                    } else {
                        Poll::Pending
                    }
                }
                // TODO: separate function for handling Handlers events
                // TODO: better match `answers` to `requests`?
                InternalEvent::Handler(peer_id, event) => handler_event(self, peer_id, event),
                InternalEvent::AskNodeType(peer_id)
                    if self
                        .get_peer_or_insert(&peer_id)
                        .borrow()
                        .node_type
                        .is_none() =>
                {
                    Poll::Ready(NetworkBehaviourAction::SendEvent {
                        peer_id,
                        event: HandlerIn::NodeTypeRequest {
                            node_type: self.node_type_data.into(),
                            user_data: self.next_query_unique_id(),
                        },
                    })
                }
                InternalEvent::AskNodeType(_) => Poll::Pending,
            };

            if let Poll::Ready(_) = answer {
                return answer;
            }
        }

        // Reads the inbound channel to handle events:
        while let Poll::Ready(event_opt) = self.inbound_rx.poll_recv(cx) {
            let action = match event_opt {
                Some(EventIn::Ping) => {
                    Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::Pong))
                }
                Some(EventIn::Handler(peer_id, event)) => {
                    Poll::Ready(NetworkBehaviourAction::SendEvent { peer_id, event })
                }
                Some(EventIn::ExecuteTask {
                    peer_id,
                    job_addr,
                    argument,
                }) => Poll::Ready(NetworkBehaviourAction::SendEvent {
                    peer_id,
                    event: HandlerIn::ExecuteTask {
                        job_addr,
                        argument,
                        user_data: self.next_query_unique_id(),
                    },
                }),
                // TODO: close the swarm if channel has been closed ?
                None => unimplemented!("Channel was closed"),
            };

            if let Poll::Ready(_) = action {
                return action;
            }
        }

        Poll::Pending
    }
}

/// Handle an event coming out of the handler.
fn handler_event<F, TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_id: PeerId,
    event: HandlerOut<QueryId>,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>>
where
    F: FnOnce() -> QueryId,
{
    let peer_rc = behaviour.get_peer_or_insert(&peer_id);

    match event {
        HandlerOut::NodeTypeRequest {
            node_type,
            request_id,
        } => node_type_request(behaviour, peer_rc, peer_id, node_type, request_id),
        HandlerOut::NodeTypeAnswer { node_type, .. } => {
            node_type_answer(behaviour, peer_rc, peer_id, node_type)
        }
        HandlerOut::NotMine { .. } => not_mine(&mut behaviour.node_type_data, peer_rc, peer_id),
        HandlerOut::ManagerRequest {
            worker_specs,
            request_id,
        } => manager_request(behaviour, peer_rc, peer_id, worker_specs, request_id, event),
        HandlerOut::ManagerAnswer { accepted, .. } => {
            manager_answer(behaviour, peer_rc, peer_id, accepted, event)
        }
        HandlerOut::NotMine { .. } => {
            match behaviour.node_type_data {
                NodeTypeData::Manager(ref mut data) => {
                    if data.workers.remove(&peer_id).is_some() {
                        behaviour.inject_generate_event(EventOut::WorkerBye(peer_id));
                    }
                }
                NodeTypeData::Worker(ref mut data) => {
                    let remove_man = if let Some(ref man) = data.manager {
                        man.borrow().peer_id == peer_id
                    } else {
                        false
                    };

                    if remove_man {
                        data.manager.take();
                        behaviour.inject_generate_event(EventOut::ManagerBye(peer_id));
                    }
                }
            }

            Poll::Pending
        }
        HandlerOut::ManagerBye { request_id } => {
            let evt = match behaviour.node_type_data {
                NodeTypeData::Manager(ref mut data) => {
                    if data.workers.get(&peer_id).is_some() {
                        data.workers.remove(&peer_id);
                        EventOut::WorkerBye(peer_id.clone())
                    } else {
                        EventOut::NotMyWorker(
                            peer_id.clone(),
                            HandlerOut::ManagerBye {
                                request_id: request_id.clone_dangerous(),
                            },
                        )
                    }
                }
                NodeTypeData::Worker(ref mut data) => {
                    if let Some(ref manager) = data.manager {
                        if manager.borrow().peer_id == peer_id {
                            EventOut::ManagerBye(peer_id.clone())
                        } else {
                            EventOut::NotMyManager(
                                peer_id.clone(),
                                HandlerOut::ManagerBye {
                                    request_id: request_id.clone_dangerous(),
                                },
                            )
                        }
                    } else {
                        EventOut::NotMyManager(
                            peer_id.clone(),
                            HandlerOut::ManagerBye {
                                request_id: request_id.clone_dangerous(),
                            },
                        )
                    }
                }
            };
            behaviour.inject_generate_event(evt);

            Poll::Ready(NetworkBehaviourAction::SendEvent {
                peer_id,
                event: HandlerIn::ManagerByeAnswer { request_id },
            })
        }
        HandlerOut::ExecuteTask {
            job_addr,
            argument,
            request_id,
        } => {
            let peer_id_clone = peer_id.clone();
            let send_not_mine = |request_id| {
                Poll::Ready(NetworkBehaviourAction::SendEvent {
                    peer_id: peer_id_clone,
                    event: HandlerIn::NotMine { request_id },
                })
            };
            let clone_evt = |request_id| HandlerOut::ExecuteTask {
                job_addr,
                argument,
                request_id,
            };

            let (evt, res) = if let NodeTypeData::Worker(ref data) = behaviour.node_type_data {
                if let Some(ref man) = data.manager {
                    if man.borrow().peer_id == peer_id {
                        (
                            EventOut::Handler(peer_id, clone_evt(request_id)),
                            Poll::Pending,
                        )
                    } else {
                        (
                            EventOut::NotMyManager(
                                peer_id,
                                clone_evt(request_id.clone_dangerous()),
                            ),
                            send_not_mine(request_id),
                        )
                    }
                } else {
                    (
                        EventOut::NotMyManager(peer_id, clone_evt(request_id.clone_dangerous())),
                        send_not_mine(request_id),
                    )
                }
            } else {
                (
                    EventOut::MsgForIncorrectNodeType {
                        peer_id,
                        expected_type: NodeType::Worker,
                        event: clone_evt(request_id.clone_dangerous()),
                    },
                    send_not_mine(request_id),
                )
            };
            behaviour.inject_generate_event(evt);

            res
        }
        HandlerOut::TaskResult { .. } => {
            let evt = if let NodeTypeData::Manager(ref data) = behaviour.node_type_data {
                if data.workers.get(&peer_id).is_some() {
                    EventOut::Handler(peer_id, event)
                } else {
                    EventOut::NotMyWorker(peer_id, event)
                }
            } else {
                EventOut::MsgForIncorrectNodeType {
                    peer_id,
                    expected_type: NodeType::Manager,
                    event,
                }
            };
            behaviour.inject_generate_event(evt);

            Poll::Pending
        } /*
          _ => {
              behaviour.inject_generate_event(EventOut::Handler(peer_id, event));
              Poll::Pending
          }
          */
    }
}

// TODO: stop using behaviour directly ?
// TODO: tests

/// A node has advertised its node type via [`worker::NodeTypeRequest`].
fn node_type_request<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    node_type: NodeType,
    request_id: RequestId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    // We act as NodeTypeAnswer to register the peers type,
    // and then answer with our own type.
    if let Poll::Ready(action) = node_type_answer(behaviour, peer_rc, peer_id, node_type) {
        behaviour.inject_behaviour_action(action);
    }

    Poll::Ready(NetworkBehaviourAction::SendEvent {
        peer_id,
        event: HandlerIn::NodeTypeAnswer {
            node_type: (&behaviour.node_type_data).into(),
            request_id,
        },
    })
}

/// A node has advertised its node type (via [`worker::NodeTypeRequest`] or
/// [`worker::NodeTypeAnswer`]).
fn node_type_answer<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    node_type: NodeType,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    let mut peer = peer_rc.borrow_mut();

    if let Some(ref previous) = peer.node_type {
        let previous = previous.into();
        if previous != node_type.into() {
            Poll::Ready(NetworkBehaviourAction::GenerateEvent(
                EventOut::PeerGivesDifferentNodeType {
                    peer_id,
                    previous,
                    new: node_type,
                },
            ))
        } else {
            Poll::Pending
        }
    } else {
        let request_man = if let (NodeType::Manager, NodeTypeData::Worker(data)) =
            (node_type, behaviour.node_type_data)
        {
            if data.manager.is_none()
                && data
                    .config
                    .is_manager_authorized(Some(&peer.peer_id), &peer.addrs_as_vec()[..])
            {
                Some(data.specs)
            } else {
                None
            }
        } else {
            None
        };

        peer.node_type = Some(node_type.into());
        behaviour.inject_generate_event(EventOut::PeerHasNewType(peer_id.clone(), node_type));

        if let Some(worker_specs) = request_man {
            Poll::Ready(NetworkBehaviourAction::SendEvent {
                peer_id,
                event: HandlerIn::ManagerRequest {
                    worker_specs,
                    user_data: behaviour.next_query_unique_id(),
                },
            })
        } else {
            Poll::Pending
        }
    }
}

/// We received a [`worker::NotMine`].
fn not_mine<TSubstream>(
    node_type_data: &mut NodeTypeData,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    match node_type_data {
        NodeTypeData::Worker(data) => {
            if let Some(man) = data.manager {
                if *peer_rc == *man {
                    data.manager = None;
                    Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::WorkerBye(
                        peer_id,
                    )))
                } else {
                    Poll::Pending
                }
            } else {
                Poll::Pending
            }
        }
        NodeTypeData::Manager(data) => {
            if data.workers.remove(&peer_id).is_some() {
                Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::WorkerBye(
                    peer_id,
                )))
            } else {
                Poll::Pending
            }
        }
    }
}

/// We received a [`worker::ManagerRequest`].
fn manager_request<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    worker_specs: WorkerSpecs,
    request_id: RequestId,
    event: HandlerOut<QueryId>,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    if let NodeTypeData::Manager(ref mut data) = behaviour.node_type_data {
        // TODO: more conditions for accepting workers ?
        // TODO: limit ?
        if let Some(NodeTypeContainer::Worker(ref mut specs_opt)) = peer_rc.borrow_mut().node_type {
            // TODO: what should be done if some specs are already known ?
            *specs_opt = Some(worker_specs);
            data.workers.insert(peer_id.clone(), peer_rc.clone());
            behaviour.inject_generate_event(EventOut::WorkerNew(peer_id.clone()));
            Poll::Ready(NetworkBehaviourAction::SendEvent {
                peer_id,
                event: HandlerIn::ManagerAnswer {
                    accepted: true,
                    request_id,
                },
            })
        } else {
            Poll::Ready(NetworkBehaviourAction::GenerateEvent(
                EventOut::MsgFromIncorrectNodeType {
                    peer_id,
                    known_type: peer_rc.borrow().node_type_into(),
                    expected_type: NodeType::Worker,
                    event,
                },
            ))
        }
    } else {
        // If we aren't a Manager, we have to refuse such requests.
        behaviour.inject_generate_event(EventOut::MsgForIncorrectNodeType {
            peer_id: peer_id.clone(),
            expected_type: NodeType::Manager,
            event: HandlerOut::ManagerRequest {
                worker_specs,
                request_id: request_id.clone_dangerous(),
            },
        });
        Poll::Ready(NetworkBehaviourAction::SendEvent {
            peer_id,
            event: HandlerIn::ManagerAnswer {
                accepted: false,
                request_id,
            },
        })
    }
}

/// We received a [`worker::ManagerAnswer`].
fn manager_answer<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    accepted: bool,
    event: HandlerOut<QueryId>,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    let peer_id_clone = peer_id.clone();
    let (evt, send_bye) = if let NodeTypeData::Worker(ref mut data) = behaviour.node_type_data {
        match (accepted, data.manager, peer_rc.borrow().node_type_into()) {
            (true, None, Some(NodeType::Manager)) => {
                if data
                    .config
                    .is_manager_authorized(Some(&peer_id), &peer_rc.borrow().addrs_as_vec()[..])
                {
                    data.manager = Some(peer_rc.clone());
                    (Some(EventOut::ManagerNew(peer_id)), false)
                } else {
                    (Some(EventOut::ManagerUnauthorized(peer_id)), true)
                }
            }
            (accepted, Some(manager), Some(NodeType::Manager)) => {
                if *manager == *peer_rc {
                    behaviour.inject_send_to_peer_event(
                        peer_id,
                        HandlerIn::ManagerPing {
                            user_data: behaviour.next_query_unique_id(),
                        },
                    );
                    (None, false)
                } else {
                    (Some(EventOut::ManagerAlreadyHasOne(peer_id)), accepted)
                }
            }
            (false, None, Some(NodeType::Manager)) => {
                (Some(EventOut::ManagerRefused(peer_id)), false)
            }
            (accepted, _, Some(NodeType::Worker)) => (
                Some(EventOut::MsgFromIncorrectNodeType {
                    peer_id,
                    known_type: peer_rc.borrow().node_type_into(),
                    expected_type: NodeType::Manager,
                    event,
                }),
                accepted,
            ),
            (accepted, _, None) => (
                Some(EventOut::MsgFromIncorrectNodeType {
                    peer_id,
                    known_type: None,
                    expected_type: NodeType::Manager,
                    event,
                }),
                accepted,
            ),
        }
    } else {
        (
            Some(EventOut::MsgForIncorrectNodeType {
                peer_id,
                expected_type: NodeType::Worker,
                event,
            }),
            accepted,
        )
    };

    if send_bye {
        if let Some(evt) = evt {
            behaviour.inject_generate_event(evt);
        }

        Poll::Ready(NetworkBehaviourAction::SendEvent {
            peer_id: peer_id_clone,
            event: HandlerIn::ManagerBye {
                user_data: behaviour.next_query_unique_id(),
            },
        })
    } else if let Some(evt) = evt {
        Poll::Ready(NetworkBehaviourAction::GenerateEvent(evt))
    } else {
        Poll::Pending
    }
}
