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
//! When extending [`InternalEvent`] or [`handler::EventOut`], update the `poll` method
//! from [`BalthBehaviour`].
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
    collections::{HashMap, VecDeque},
    error,
    fmt::Debug,
    marker::PhantomData,
    task::{Context, Poll},
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub mod handler;
use handler::Balthandler;
use misc::NodeType;

const CHANNEL_SIZE: usize = 1024;

/// Event injected into [`BalthBehaviour`] from either [`Balthandler`] and from outside (other
/// [`NetworkBehaviour`]s, etc.
#[derive(Debug)]
enum InternalEvent<TUserData> {
    /// Event created when the [`Mdns`](`libp2p::mdns::Mdns`) discovers a new peer at given multiaddress.
    Mdns(PeerId, Multiaddr),
    /// Event originating from [`Balthandler`].
    Handler(PeerId, handler::EventOut<TUserData>),
    /// Request a node type.
    /// TODO: delete and delegate to outside, or default here?
    AskNodeType(PeerId),
    /// Generates an event towards the Swarm.
    GenerateEvent(EventOut),
}

/// TODO: doc
#[derive(Debug)]
pub enum EventIn {
    Ping,
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
    Handler(PeerId, handler::EventOut<QueryId>),
}

/// Type to identify our queries within [`Balthandler`] to link answers to queries.
pub type QueryId = usize;

/// Peer data as used by [`BalthBehaviour`].
#[derive(Clone, Debug)]
struct Peer {
    peer_id: PeerId,
    /// Known addresses
    addrs: Vec<Multiaddr>,
    /// If there's an open connection, the connection infos
    endpoint: Option<ConnectedPoint>,
    /// Has it already been dialed at least once?
    /// TODO: use a date to recheck periodically ?
    dialed: bool,
    /// Defines the node type if it is known.
    node_type: Option<NodeType>,
}

impl Peer {
    pub fn new(peer_id: PeerId) -> Self {
        Peer {
            peer_id,
            addrs: Vec::new(),
            endpoint: None,
            dialed: false,
            node_type: None,
        }
    }
}

/// The [`NetworkBehaviour`] to manage the networking of the **Balthazar** node.
pub struct BalthBehaviour<TSubstream> {
    inbound_rx: Receiver<EventIn>,
    // TODO: should the node_type be kept here, what happens if it changes elsewhere?
    node_type: NodeType,
    peers: HashMap<PeerId, Peer>,
    events: VecDeque<InternalEvent<QueryId>>,
    next_query_unique_id: QueryId,
    _marker: PhantomData<TSubstream>,
}

impl<TSubstream> BalthBehaviour<TSubstream> {
    /// Creates a new [`BalthBehaviour`] and returns a [`Sender`] channel to communicate with it from
    /// the exterior of the Swarm.
    pub fn new(node_type: NodeType) -> (Self, Sender<EventIn>) {
        let (tx, inbound_rx) = channel(CHANNEL_SIZE);

        (
            BalthBehaviour {
                inbound_rx,
                node_type,
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

    fn inject_handler_event(&mut self, peer_id: PeerId, handler_evt: handler::EventOut<QueryId>) {
        self.events
            .push_front(InternalEvent::Handler(peer_id, handler_evt));
    }

    fn inject_generate_event(&mut self, evt: EventOut) {
        self.events.push_front(InternalEvent::GenerateEvent(evt));
    }

    fn get_peer_or_insert(&mut self, peer_id: &PeerId) -> &mut Peer {
        self.peers
            .entry(peer_id.clone())
            .or_insert_with(|| Peer::new(peer_id.clone()))
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
            .map(|p| &p.addrs)
            .unwrap_or(&Vec::new())
            .clone()
    }

    fn inject_connected(&mut self, peer_id: PeerId, endpoint: ConnectedPoint) {
        let mut peer = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| Peer::new(peer_id.clone()));
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

            peer.endpoint = Some(endpoint.clone());

            self.inject_generate_event(EventOut::PeerConnected(peer_id, endpoint));
        }
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId, endpoint: ConnectedPoint) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            if peer.endpoint.take().is_some() {
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
                InternalEvent::Mdns(peer_id, _) => {
                    // TODO: use the multiaddr ?
                    let mut peer = self.get_peer_or_insert(&peer_id);
                    if !peer.dialed {
                        peer.dialed = true;
                        Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id })
                    } else {
                        Poll::Pending
                    }
                }
                // TODO: separate function for handling Handlers events
                InternalEvent::Handler(peer_id, event) => {
                    let mut peer = self.get_peer_or_insert(&peer_id);
                    match event {
                        handler::EventOut::NodeTypeRequest { request_id } => {
                            Poll::Ready(NetworkBehaviourAction::SendEvent {
                                peer_id: peer.peer_id.clone(),
                                event: handler::EventIn::NodeTypeAnswer {
                                    node_type: self.node_type,
                                    request_id,
                                },
                            })
                        }
                        handler::EventOut::NodeTypeAnswer { node_type, .. } => {
                            if let Some(ref known_node_type) = peer.node_type {
                                if *known_node_type != node_type {
                                    eprintln!("E --- Peer `{:?}` answered a different node_type `{:?}` than before `{:?}`", peer.peer_id, known_node_type, node_type);
                                }
                            } else {
                                peer.node_type = Some(node_type);
                                self.inject_generate_event(EventOut::PeerHasNewType(
                                    peer_id, node_type,
                                ));
                            }
                            Poll::Pending
                        }
                        _ => {
                            self.inject_generate_event(EventOut::Handler(peer_id, event));
                            Poll::Pending
                        }
                    }
                }
                InternalEvent::AskNodeType(peer_id)
                    if self.get_peer_or_insert(&peer_id).node_type.is_none() =>
                {
                    Poll::Ready(NetworkBehaviourAction::SendEvent {
                        peer_id,
                        event: handler::EventIn::NodeTypeRequest {
                            user_data: self.next_query_unique_id(),
                        },
                    })
                }
                InternalEvent::AskNodeType(_) => Poll::Pending,
                InternalEvent::GenerateEvent(event) => {
                    Poll::Ready(NetworkBehaviourAction::GenerateEvent(event))
                }
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
