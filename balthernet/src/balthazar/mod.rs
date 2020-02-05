//! Provides [`BalthBehaviour`], a [`NetworkBehaviour`] for use in [`libp2p`].
//!
//! This module is greatly inspired from [`libp2p::kad::Kademlia`].
//!
//! ## Procedure when adding events or new kinds of messages
//!
//! If there are new kinds of events that can be received, add them to [`BalthBehaviourKindOfEvent`].
//!
//! Have a look at the [`handler`] module description to make the necessary updates.
//!
//! When extending [`BalthBehaviourKindOfEvent`] or [`handler::BalthandlerEventOut`], update the `poll` method
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

pub mod handler;
use handler::{Balthandler, BalthandlerEventIn, BalthandlerEventOut};
use misc::NodeType;

/// Wrapper event to add the [`PeerId`] of the peer linked to the event.
#[derive(Debug)]
pub struct GenericEvent<T> {
    peer_id: PeerId,
    event: T,
}

impl<T> GenericEvent<T> {
    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn event(&self) -> &T {
        &self.event
    }
}

impl<T> GenericEvent<T> {
    pub fn new(peer_id: PeerId, event: T) -> Self {
        GenericEvent { peer_id, event }
    }

    /*
     * TODO: Keep ?
    pub fn new_from_custom(peer_id: PeerId, event: TCustomEvent) -> Self {
        GenericEvent {
            peer_id,
            event: BalthBehaviourKindOfEvent::Custom(event),
        }
    }
    */
}

/// Event injected into [`BalthBehaviour`] from either [`Balthandler`] and from outside (other
/// [`NetworkBehaviour`]s, etc.
#[derive(Debug)]
pub enum BalthBehaviourKindOfEvent<TUserData> {
    /// Event created when the [`Mdns`](`libp2p::mdns::Mdns`) discovers a new peer at given multiaddress.
    Mdns(Multiaddr),
    /// Event originating from [`Balthandler`].
    Handler(BalthandlerEventOut<TUserData>),
    /*
     * TODO: useful ?
    /// Other events from the another part of the node (i.e. usually from outside of the networking part of the system).
    Custom(TCustomEvent),
    */
    /// Request a node type
    /// TODO: delete and delegate to outside, or default here?
    AskNodeType,
}

/// Type to identify our queries within [`Balthandler`] to link answers to queries.
pub type QueryId = usize;

/// Events used internally by [`BalthBehaviour`].
pub type BalthBehaviourEvent = GenericEvent<BalthBehaviourKindOfEvent<QueryId>>;

/*
async fn dummy<T: Clone>(
    node_type: NodeType<T>,
    mut tx: Sender<BalthandlerEventIn<QueryId>>,
    mut rx: Receiver<BalthandlerEventOut<QueryId>>,
) {
    println!("dummy spawned");
    match node_type {
        NodeType::Manager => {
            while let Some(BalthandlerEventOut::ManagerRequest { request_id }) = rx.recv().await {
                println!("dummy man accepts");
                tx.send(BalthandlerEventIn::ManagerAnswer {
                    accepted: true,
                    request_id,
                })
                .await
                .unwrap()
            }
        }
        NodeType::Worker(_) => {
            interval(Duration::from_secs(5))
                .for_each(|_| {
                    println!("dummy asks man");
                    let mut tx = tx.clone();
                    async move { tx.send(BalthandlerEventIn::Dummy).await.unwrap() }
                })
                .await
        } // _ => unimplemented!(),
    }
}
*/

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
    node_type: Option<NodeType<()>>,
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

type BehaviourNodeType = NodeType<Option<PeerId>>;

/// The [`NetworkBehaviour`] to manage the networking of the **Balthazar** node.
pub struct BalthBehaviour<TSubstream> {
    // dummy_tx: Sender<BalthandlerEventOut<QueryId>>,
    // dummy_rx: Receiver<BalthandlerEventIn<QueryId>>,
    // TODO: should the node_type be kept here, what happens if it changes elsewhere?
    node_type: BehaviourNodeType,
    peers: HashMap<PeerId, Peer>,
    events: VecDeque<BalthBehaviourEvent>,
    next_query_unique_id: QueryId,
    _marker: PhantomData<TSubstream>,
}

impl<TSubstream> BalthBehaviour<TSubstream> {
    pub fn new<T: Clone>(node_type: NodeType<T>) -> Self {
        let node_type: BehaviourNodeType = node_type.map(|_| None);

        /*
        let (dummy_tx, rx_d) = channel(1024);
        let (tx_d, dummy_rx) = channel(10);
        tokio::spawn(dummy(node_type.clone(), tx_d, rx_d));
        */

        BalthBehaviour {
            node_type,
            // dummy_tx,
            // dummy_rx,
            peers: HashMap::new(),
            events: VecDeque::new(),
            next_query_unique_id: 0,
            _marker: PhantomData,
        }
    }

    /// Gets a new unique identifier for a new message request generates a new one.
    pub fn next_query_unique_id(&mut self) -> QueryId {
        let old = self.next_query_unique_id;
        self.next_query_unique_id += 1;
        old
    }

    pub fn events(&self) -> &VecDeque<BalthBehaviourEvent> {
        &self.events
    }

    pub fn events_mut(&mut self) -> &mut VecDeque<BalthBehaviourEvent> {
        &mut self.events
    }
}

impl<TSubstream> NetworkBehaviour for BalthBehaviour<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type ProtocolsHandler = Balthandler<TSubstream, QueryId>;
    type OutEvent = BalthBehaviourEvent;

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
            eprintln!("Connected for {:?} : {:?}", peer_id, endpoint);
            peer.endpoint = Some(endpoint);
        }

        // If the node_type is unknown, plans to send a request:
        if peer.node_type.is_none() {
            self.events.push_front(BalthBehaviourEvent::new(
                peer_id,
                BalthBehaviourKindOfEvent::AskNodeType,
            ));
        }
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId, endpoint: ConnectedPoint) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            if peer.endpoint.take().is_some() {
                eprintln!("Disconnected for {:?} : {:?}", peer_id, endpoint);
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
        eprintln!(
            "Event injected in Behaviour from Handler {:?} : {:?}",
            peer_id, event
        );
        self.events.push_front(BalthBehaviourEvent::new(
            peer_id,
            BalthBehaviourKindOfEvent::Handler(event),
        ))
    }

    // TODO: break this function into one or several external functions for clearer parsing ?
    // If yes, update the module doc.
    fn poll(
        &mut self,
        _cx: &mut Context,
        _params: &mut impl PollParameters
) -> Poll<NetworkBehaviourAction<<<Self::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::InEvent, Self::OutEvent>>{
        while let Some(e) = self.events.pop_back() {
            let mut peer = self
                .peers
                .entry(e.peer_id.clone())
                .or_insert_with(|| Peer::new(e.peer_id.clone()));
            let answer = match e.event {
                BalthBehaviourKindOfEvent::Mdns(_) if !peer.dialed => {
                    peer.dialed = true;
                    Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id: e.peer_id })
                }
                BalthBehaviourKindOfEvent::Mdns(_) => Poll::Pending,
                BalthBehaviourKindOfEvent::Handler(BalthandlerEventOut::NodeTypeRequest {
                    request_id,
                }) => Poll::Ready(NetworkBehaviourAction::SendEvent {
                    peer_id: peer.peer_id.clone(),
                    event: BalthandlerEventIn::NodeTypeAnswer {
                        node_type: self.node_type.clone().map(|_| ()),
                        request_id,
                    },
                }),
                BalthBehaviourKindOfEvent::Handler(BalthandlerEventOut::NodeTypeAnswer {
                    node_type,
                    ..
                }) => {
                    if let Some(ref known_node_type) = peer.node_type {
                        if *known_node_type != node_type {
                            eprintln!("Peer `{:?}` answered a different node_type `{:?}` than before `{:?}`", peer.peer_id, known_node_type, node_type);
                        }
                    } else {
                        eprintln!("Peer `{:?}` has a new type `{:?}`", peer.peer_id, node_type);
                        peer.node_type = Some(node_type);
                    }
                    Poll::Pending
                }
                // Forward event to other parts of system.
                /*
                BalthBehaviourKindOfEvent::Handler(event) => {
                    let mut tx = self.dummy_tx.clone();
                    tokio::spawn(async move {
                        tx.send(event).await.unwrap();
                    });
                    Poll::Pending
                    /* Poll::Ready(NetworkBehaviourAction::SendEvent {
                        peer_id: e.peer_id,
                        event: BalthandlerEventIn::ManagerAnswer {
                            accepted: true,
                            request_id,
                        },
                    })*/
                }
                */
                BalthBehaviourKindOfEvent::AskNodeType if peer.node_type.is_none() => {
                    Poll::Ready(NetworkBehaviourAction::SendEvent {
                        peer_id: e.peer_id,
                        event: BalthandlerEventIn::NodeTypeRequest {
                            user_data: self.next_query_unique_id(),
                        },
                    })
                }
                _ => {
                    // println!("Forwarding event {:?}", e);
                    Poll::Ready(NetworkBehaviourAction::GenerateEvent(e))
                }
            };

            if let Poll::Ready(_) = answer {
                return answer;
            }
        }
        /*
        if let NodeType::Worker(Some(ref manager_id)) = self.node_type {
            match self.dummy_rx.poll_recv(cx) {
                Poll::Ready(Some(BalthandlerEventIn::Dummy)) => {
                    Poll::Ready(NetworkBehaviourAction::SendEvent {
                        peer_id: manager_id.clone(),
                        event: BalthandlerEventIn::ManagerRequest {
                            user_data: self.next_query_unique_id(),
                        },
                    })
                }
                Poll::Ready(Some(event)) => Poll::Ready(NetworkBehaviourAction::SendEvent {
                    peer_id: manager_id.clone(),
                    event,
                }),
                Poll::Ready(None) => panic!("Dummy has died"),
                Poll::Pending => Poll::Pending,
            }
        } else {
            Poll::Pending
        }
        */
        Poll::Pending
    }
}
