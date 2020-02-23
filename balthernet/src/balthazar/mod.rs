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
use proto::worker;
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
    /// When a peer sends us a message, but we aren't in a worker-manager relationship.
    NotMine(PeerId, HandlerOut<QueryId>),
    /// Events created by [`Balthandler`] which are not handled directly in [`BalthBehaviour`]
    Handler(PeerId, HandlerOut<QueryId>),
    /// A new worker is now managed by us.
    WorkerNew(PeerId),
    /// When a worker stops being managed by us.
    WorkerBye(PeerId),
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
    TasksExecute(HashMap<Vec<u8>, worker::TaskExecute>),
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
        self.node_type.as_ref().map(|t| t.into())
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

    /// If we are a worker: checks if given peer is our manager,
    /// if we are a manager: checks if given peer is one of our workers.
    fn is_in_relationship_with(&self, peer_rc: Rc<RefCell<Peer>>) -> bool {
        match &self.node_type_data {
            NodeTypeData::Manager(data) => data.workers.get(&peer_rc.borrow().peer_id).is_some(),
            NodeTypeData::Worker(data) => data.manager.as_ref().map_or(false, |m| *m == peer_rc),
        }
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
                            node_type: (&self.node_type_data).into(),
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
///
/// > **Note when adding new events:** For request messages, the handling function has to return
/// > a `Poll::Ready(NetworkBehaviourAction::SendEvent {..})` value.
/// > To make sure of this (and panic if it's not the case),
/// > wrap the handler function into [`ensure_answer`].
fn handler_event<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_id: PeerId,
    event: HandlerOut<QueryId>,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    let peer_rc = behaviour.get_peer_or_insert(&peer_id);

    match event {
        HandlerOut::NodeTypeRequest {
            node_type,
            request_id,
        } => wrap_answer(
            peer_id.clone(),
            node_type_request(behaviour, peer_rc, peer_id, node_type, request_id),
        ),
        HandlerOut::NodeTypeAnswer { node_type, .. } => {
            node_type_answer(behaviour, peer_rc, peer_id, node_type)
        }
        HandlerOut::NotMine { .. } => not_mine(&mut behaviour.node_type_data, peer_rc, peer_id),
        HandlerOut::ManagerRequest {
            worker_specs,
            request_id,
        } => wrap_answer(
            peer_id.clone(),
            manager_request(behaviour, peer_rc, peer_id, worker_specs, request_id),
        ),
        HandlerOut::ManagerAnswer {
            accepted,
            user_data,
        } => manager_answer(behaviour, peer_rc, peer_id, accepted, user_data),
        HandlerOut::ManagerBye { request_id } => wrap_answer(
            peer_id.clone(),
            manager_bye(behaviour, peer_rc, peer_id, request_id),
        ),
        HandlerOut::ManagerPing { request_id } => {
            let id_clone = request_id.clone_dangerous();
            wrap_answer(
                peer_id.clone(),
                need_relashionship_with(
                    behaviour,
                    peer_rc,
                    peer_id,
                    request_id,
                    |_, r| manager_ping(r),
                    || HandlerOut::ManagerPing {
                        request_id: id_clone,
                    },
                ),
            )
        }
        HandlerOut::TasksExecute { tasks, request_id } => {
            let id_clone = request_id.clone_dangerous();
            wrap_answer(
                peer_id.clone(),
                need_relashionship_with(
                    behaviour,
                    peer_rc,
                    peer_id,
                    request_id,
                    |b, r| tasks_execute(b, tasks, r),
                    || HandlerOut::TasksExecute {
                        tasks: HashMap::new(),
                        request_id: id_clone,
                    },
                ),
            )
        }
        _ => {
            behaviour.inject_generate_event(EventOut::Handler(peer_id, event));
            Poll::Pending
        }
    }
}

// TODO: stop using behaviour directly ?
// TODO: tests

fn wrap_answer(
    peer_id: PeerId,
    event: HandlerIn<QueryId>,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    Poll::Ready(NetworkBehaviourAction::SendEvent { peer_id, event })
}

/// Check if the peer is in relationship with us, if yes does the given action,
/// otherwise sends [`worker::NotMine`] to the peer.
fn need_relashionship_with<TSubstream, F, G>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    request_id: RequestId,
    action_if_in_relashionship: F,
    clone_event: G,
) -> HandlerIn<QueryId>
where
    F: FnOnce(&mut BalthBehaviour<TSubstream>, RequestId) -> HandlerIn<QueryId>,
    G: FnOnce() -> HandlerOut<QueryId>,
{
    if behaviour.is_in_relationship_with(peer_rc) {
        action_if_in_relashionship(behaviour, request_id)
    } else {
        behaviour.inject_generate_event(EventOut::NotMine(peer_id, clone_event()));

        HandlerIn::NotMine { request_id }
    }
}

/// If the two nodes are in a relationship, breaks it, otherwise does nothing.
fn break_worker_manager_relationship(
    node_type_data: &mut NodeTypeData,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    match node_type_data {
        NodeTypeData::Worker(data) => {
            if let Some(man) = &data.manager {
                if peer_rc == *man {
                    data.manager = None;
                    Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::ManagerBye(
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

/// A node has advertised its node type via [`worker::NodeTypeRequest`].
fn node_type_request<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    node_type: NodeType,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    // We act as NodeTypeAnswer to register the peers type,
    // and then answer with our own type.
    if let Poll::Ready(action) = node_type_answer(behaviour, peer_rc, peer_id, node_type) {
        behaviour.inject_behaviour_action(action);
    }

    HandlerIn::NodeTypeAnswer {
        node_type: (&behaviour.node_type_data).into(),
        request_id,
    }
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
        if previous != node_type {
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
            (node_type, &behaviour.node_type_data)
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

// TODO: function alias ?
/// We received a [`worker::NotMine`].
fn not_mine(
    node_type_data: &mut NodeTypeData,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    break_worker_manager_relationship(node_type_data, peer_rc, peer_id)
}

/// We received a [`worker::ManagerRequest`].
fn manager_request<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    worker_specs: WorkerSpecs,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    if let NodeTypeData::Manager(ref mut data) = behaviour.node_type_data {
        // TODO: more conditions for accepting workers ?
        // TODO: limit ?
        if let Some(NodeTypeContainer::Worker(ref mut specs_opt)) = peer_rc.borrow_mut().node_type {
            // TODO: what should be done if some specs are already known ?
            *specs_opt = Some(worker_specs);
            data.workers.insert(peer_id.clone(), peer_rc.clone());
            behaviour.inject_generate_event(EventOut::WorkerNew(peer_id));
            HandlerIn::ManagerAnswer {
                accepted: true,
                request_id,
            }
        } else {
            behaviour.inject_generate_event(EventOut::MsgFromIncorrectNodeType {
                peer_id,
                known_type: peer_rc.borrow().node_type_into(),
                expected_type: NodeType::Worker,
                event: HandlerOut::ManagerRequest {
                    worker_specs,
                    request_id: request_id.clone_dangerous(),
                },
            });
            HandlerIn::ManagerAnswer {
                accepted: false,
                request_id,
            }
        }
    } else {
        // If we aren't a Manager, we have to refuse such requests.
        behaviour.inject_generate_event(EventOut::MsgForIncorrectNodeType {
            peer_id,
            expected_type: NodeType::Manager,
            event: HandlerOut::ManagerRequest {
                worker_specs,
                request_id: request_id.clone_dangerous(),
            },
        });
        HandlerIn::ManagerAnswer {
            accepted: false,
            request_id,
        }
    }
}

/// We received a [`worker::ManagerAnswer`].
fn manager_answer<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    accepted: bool,
    user_data: QueryId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    let peer_id_clone = peer_id.clone();
    let (evt, send_bye) = if let NodeTypeData::Worker(ref mut data) = behaviour.node_type_data {
        match (accepted, &data.manager, peer_rc.borrow().node_type_into()) {
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
                if *manager == peer_rc {
                    let user_data = behaviour.next_query_unique_id();
                    behaviour
                        .inject_send_to_peer_event(peer_id, HandlerIn::ManagerPing { user_data });
                    (None, false)
                } else {
                    (Some(EventOut::ManagerAlreadyHasOne(peer_id)), accepted)
                }
            }
            (false, None, Some(NodeType::Manager)) => {
                (Some(EventOut::ManagerRefused(peer_id)), false)
            }
            (accepted, _, _) => (
                Some(EventOut::MsgFromIncorrectNodeType {
                    peer_id,
                    known_type: peer_rc.borrow().node_type_into(),
                    expected_type: NodeType::Manager,
                    event: HandlerOut::ManagerAnswer {
                        accepted,
                        user_data,
                    },
                }),
                accepted,
            ),
        }
    } else {
        (
            Some(EventOut::MsgForIncorrectNodeType {
                peer_id,
                expected_type: NodeType::Worker,
                event: HandlerOut::ManagerAnswer {
                    accepted,
                    user_data,
                },
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

/// We received a [`worker::ManagerBye`].
fn manager_bye<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    peer_rc: Rc<RefCell<Peer>>,
    peer_id: PeerId,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    let evt = break_worker_manager_relationship(&mut behaviour.node_type_data, peer_rc, peer_id);
    if let Poll::Ready(evt) = evt {
        behaviour.inject_behaviour_action(evt);
    }

    HandlerIn::Ack { request_id }
}

/// We received a [`worker::ManagerPing`].
fn manager_ping(request_id: RequestId) -> HandlerIn<QueryId> {
    HandlerIn::ManagerPong { request_id }
}

/// We received a [`worker::TasksExecute`].
fn tasks_execute<TSubstream>(
    behaviour: &mut BalthBehaviour<TSubstream>,
    tasks: HashMap<Vec<u8>, worker::TaskExecute>,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    behaviour.inject_generate_event(EventOut::TasksExecute(tasks));

    HandlerIn::Ack { request_id }
}
