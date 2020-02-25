//! Provides [`BalthBehaviour`], a [`NetworkBehaviour`] for use in [`libp2p`].
//!
//! This module is greatly inspired from [`libp2p::kad::Kademlia`].
//!
//! ## Procedure when adding events or new kinds of messages
//!
//! TODO: update explaination
//!
//! If there are new kinds of events that can be received, add them to [`InternalEvent`].
//!
//! Have a look at the [`handler`] module description to make the necessary updates.
//!
//! When extending [`HandlerOut`], update the `handler_event` function.
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
    collections::{HashMap, HashSet, VecDeque},
    error,
    fmt::Debug,
    sync::{Arc, RwLock},
    task::{Context, Poll},
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

mod events;
use events::*;
pub use events::{EventIn, EventOut};
pub mod handler;
use super::{ManagerConfig, WorkerConfig};
use handler::{Balthandler, EventIn as HandlerIn, EventOut as HandlerOut, RequestId};
use misc::{NodeType, NodeTypeContainer, TaskStatus, WorkerSpecs};

const CHANNEL_SIZE: usize = 1024;

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
    /// Send message to peer.
    SendMessage(PeerId, HandlerIn<QueryId>),
}

/// Type to identify our queries within [`Balthandler`] to link answers to queries.
pub type QueryId = usize;

/// The [`NetworkBehaviour`] to manage the networking of the **Balthazar** node.
pub struct BalthBehaviour {
    inbound_rx: Receiver<EventIn>,
    // TODO: should the node_type be kept here, what happens if it changes elsewhere?
    node_type_data: NodeTypeData,
    peers: HashMap<PeerId, Arc<RwLock<Peer>>>,
    events: VecDeque<InternalEvent<QueryId>>,
    next_query_unique_id: QueryId,
}

impl BalthBehaviour {
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
            },
            tx,
        )
    }

    /// If we are a worker: checks if given peer is our manager,
    /// if we are a manager: checks if given peer is one of our workers.
    fn is_in_relationship_with(&self, peer_rc: Arc<RwLock<Peer>>) -> bool {
        match &self.node_type_data {
            NodeTypeData::Manager(data) => {
                data.workers.get(&peer_rc.read().unwrap().peer_id).is_some()
            }
            NodeTypeData::Worker(data) => data.manager.as_ref().map_or(false, |m| {
                m.read().unwrap().peer_id == peer_rc.read().unwrap().peer_id
            }),
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

    fn get_peer_or_insert(&mut self, peer_id: &PeerId) -> Arc<RwLock<Peer>> {
        self.peers
            .entry(peer_id.clone())
            .or_insert_with(|| Arc::new(RwLock::new(Peer::new(peer_id.clone()))))
            .clone()
    }

    fn send_message_or_dial(
        &mut self,
        peer_id: PeerId,
        event: HandlerIn<QueryId>,
    ) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
        let peer = self.get_peer_or_insert(&peer_id);
        let mut peer = peer.write().unwrap();

        if peer.endpoint.is_some() {
            Poll::Ready(NetworkBehaviourAction::SendEvent { peer_id, event })
        } else {
            peer.pending_messages.push(event);
            Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id })
        }
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

impl NetworkBehaviour for BalthBehaviour {
    type ProtocolsHandler = Balthandler<QueryId>;
    type OutEvent = EventOut;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        // println!("New handler");
        Self::ProtocolsHandler::default()
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        // println!("Addresses of {:?} : {:?}", peer_id, addresses);
        self.peers
            .get(peer_id)
            .map(|p| p.read().unwrap().addrs_as_vec())
            .unwrap_or_else(Vec::new)
    }

    fn inject_connected(&mut self, peer_id: PeerId, endpoint: ConnectedPoint) {
        let peer = self.get_peer_or_insert(&peer_id);
        let mut peer = peer.write().unwrap();

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
            peer.pending_messages.drain(..).for_each(|msg| {
                self.events
                    .push_front(InternalEvent::SendMessage(peer_id.clone(), msg))
            });

            self.inject_generate_event(EventOut::PeerConnected(peer_id, endpoint));
        }
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId, endpoint: ConnectedPoint) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            if peer.write().unwrap().endpoint.take().is_some() {
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
        // Reads the inbound channel to handle events:
        while let Poll::Ready(event_opt) = self.inbound_rx.poll_recv(cx) {
            let action = match event_opt {
                Some(EventIn::Ping) => {
                    Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::Pong))
                }
                Some(EventIn::Handler(peer_id, event)) => {
                    Poll::Ready(NetworkBehaviourAction::SendEvent { peer_id, event })
                }
                Some(EventIn::TasksExecute(peer_id, tasks)) => {
                    let event = HandlerIn::TasksExecute {
                        tasks,
                        user_data: self.next_query_unique_id(),
                    };
                    self.send_message_or_dial(peer_id, event)
                }
                Some(EventIn::TasksPing(peer_id, task_ids)) => {
                    let event = HandlerIn::TasksPing {
                        task_ids,
                        user_data: self.next_query_unique_id(),
                    };
                    self.send_message_or_dial(peer_id, event)
                }
                Some(EventIn::TasksPong {
                    statuses,
                    request_id,
                }) => {
                    if let NodeTypeData::Worker(WorkerData {
                        manager: Some(ref manager),
                        ..
                    }) = self.node_type_data
                    {
                        Poll::Ready(NetworkBehaviourAction::SendEvent {
                            peer_id: manager.read().unwrap().peer_id.clone(),
                            event: HandlerIn::TasksPong {
                                statuses,
                                request_id,
                            },
                        })
                    } else {
                        Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::NoManager(
                            EventIn::TasksPong {
                                statuses,
                                request_id,
                            },
                        )))
                    }
                }
                Some(EventIn::TaskStatus(task_id, status)) => {
                    if let NodeTypeData::Worker(WorkerData {
                        manager: Some(ref manager),
                        ..
                    }) = self.node_type_data
                    {
                        let peer_id = manager.clone().read().unwrap().peer_id.clone();
                        let event = HandlerIn::TaskStatus {
                            task_id,
                            status,
                            user_data: self.next_query_unique_id(),
                        };
                        self.send_message_or_dial(peer_id, event)
                    } else {
                        Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::NoManager(
                            EventIn::TaskStatus(task_id, status),
                        )))
                    }
                }
                // TODO: close the swarm if channel has been closed ?
                None => unimplemented!("Channel was closed"),
            };

            if let Poll::Ready(_) = action {
                return action;
            }
        }

        // Go through the queued events and handle them:
        while let Some(internal_evt) = self.events.pop_back() {
            let answer = match internal_evt {
                InternalEvent::NetworkBehaviourAction(action) => Poll::Ready(action),
                InternalEvent::Mdns(peer_id, address) => {
                    let peer = self.get_peer_or_insert(&peer_id);
                    let mut peer = peer.write().unwrap();
                    peer.addrs.insert(address);

                    if !peer.dialed {
                        peer.dialed = true;
                        Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id })
                    } else {
                        Poll::Pending
                    }
                }
                InternalEvent::SendMessage(peer_id, event) => {
                    self.send_message_or_dial(peer_id, event)
                }
                // TODO: separate function for handling Handlers events
                // TODO: better match `answers` to `requests`?
                InternalEvent::Handler(peer_id, event) => handler_event(self, peer_id, event),
                InternalEvent::AskNodeType(peer_id)
                    if self
                        .get_peer_or_insert(&peer_id)
                        .read()
                        .unwrap()
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

        Poll::Pending
    }
}

/// Handle an event coming out of the handler.
///
/// > **Note when adding new events:** For request messages, the handling function has to return
/// > a `Poll::Ready(NetworkBehaviourAction::SendEvent {..})` value.
/// > To make sure of this (and panic if it's not the case),
/// > wrap the handler function into [`ensure_answer`].
fn handler_event(
    behaviour: &mut BalthBehaviour,
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
            needs_relationship_with(
                behaviour,
                peer_rc,
                peer_id.clone(),
                request_id,
                |_, r| wrap_answer(peer_id, manager_ping(r)),
                || HandlerOut::ManagerPing {
                    request_id: id_clone,
                },
            )
        }
        HandlerOut::TasksExecute { tasks, request_id } => {
            let id_clone = request_id.clone_dangerous();
            needs_relationship_with(
                behaviour,
                peer_rc,
                peer_id.clone(),
                request_id,
                |b, r| wrap_answer(peer_id, tasks_execute(b, tasks, r)),
                || HandlerOut::TasksExecute {
                    // TODO: actually copy tasks ?
                    tasks: HashMap::new(),
                    request_id: id_clone,
                },
            )
        }
        HandlerOut::TasksPing {
            task_ids,
            request_id,
        } => {
            let id_clone = request_id.clone_dangerous();
            needs_relationship_with(
                behaviour,
                peer_rc,
                peer_id,
                request_id,
                |_, r| tasks_ping(task_ids, r),
                || HandlerOut::TasksPing {
                    // TODO: actually copy task_ids ?
                    task_ids: Vec::new(),
                    request_id: id_clone,
                },
            )
        }
        HandlerOut::TasksPong {
            statuses,
            user_data,
        } => tasks_pong(behaviour, peer_rc, peer_id, statuses, user_data),
        HandlerOut::TasksAbord {
            task_ids,
            request_id,
        } => {
            let id_clone = request_id.clone_dangerous();
            needs_relationship_with(
                behaviour,
                peer_rc,
                peer_id.clone(),
                request_id,
                |b, r| wrap_answer(peer_id, tasks_abord(b, task_ids, r)),
                || HandlerOut::TasksAbord {
                    // TODO: actually copy task_ids ?
                    task_ids: Vec::new(),
                    request_id: id_clone,
                },
            )
        }
        HandlerOut::TaskStatus {
            task_id,
            status,
            request_id,
        } => {
            let id_clone = request_id.clone_dangerous();
            let task_id_clone = task_id.clone();
            let status_clone = status.clone();

            needs_relationship_with(
                behaviour,
                peer_rc,
                peer_id.clone(),
                request_id,
                |b, r| {
                    wrap_answer(
                        peer_id.clone(),
                        task_status(b, peer_id, task_id_clone, status_clone, r),
                    )
                },
                || HandlerOut::TaskStatus {
                    // TODO: actually copy task_ids ?
                    task_id,
                    status,
                    request_id: id_clone,
                },
            )
        }
        HandlerOut::QueryError { .. } => {
            behaviour.inject_generate_event(EventOut::Handler(peer_id, event));
            Poll::Pending
        } /*
          _ => {
              behaviour.inject_generate_event(EventOut::Handler(peer_id, event));
              Poll::Pending
          }
          */
    }
}
