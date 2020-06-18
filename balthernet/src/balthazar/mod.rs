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
use futures::channel::oneshot;
use libp2p::{
    core::connection::{ConnectionId, ListenerId},
    swarm::{
        protocols_handler::{IntoProtocolsHandler, ProtocolsHandler},
        DialPeerCondition, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler, PollParameters,
    },
    Multiaddr, PeerId,
};
use misc::job::TaskId;
use proto::worker;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    error,
    fmt::Debug,
    sync::{Arc, RwLock},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::time::DelayQueue;

mod events;
use events::*;
pub use events::{EventIn, EventOut};
pub mod handler;
use super::{ManagerConfig, WorkerConfig};
use handler::{Balthandler, EventIn as HandlerIn, EventOut as HandlerOut, RequestId};
use misc::WorkerSpecs;
use proto::{NodeType, NodeTypeContainer, TaskStatus};

pub type PeerRc = Arc<RwLock<Peer>>;

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
    // TODO: should the node_type be kept here, what happens if it changes elsewhere?
    node_type_data: NodeTypeData,
    peers: HashMap<PeerId, PeerRc>,
    events: VecDeque<InternalEvent<QueryId>>,
    next_query_unique_id: QueryId,
    /// See [`NetConfig::manager_check_interval`](`super::NetConfig::manager_check_interval`) for more information.
    manager_check_interval: Duration,
    /// See [`NetConfig::manager_timeout`](`super::NetConfig::manager_timeout`) for more information.
    manager_timeout: Duration,
    /// Queue of delays which will wake up the behaviour.
    delays: DelayQueue<()>,
    /// Tells if the system is shutting down, so we shouldn't send or accept any message
    /// anymore...
    is_shutting_down: bool,
}

impl BalthBehaviour {
    pub fn new(
        // TODO: refererences?
        node_type_conf: NodeTypeContainer<ManagerConfig, (WorkerConfig, WorkerSpecs)>,
        manager_check_interval: Duration,
        manager_timeout: Duration,
    ) -> Self {
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

        BalthBehaviour {
            node_type_data,
            peers: HashMap::new(),
            events: VecDeque::new(),
            next_query_unique_id: 0,
            manager_check_interval,
            manager_timeout,
            delays: DelayQueue::new(),
            is_shutting_down: false,
        }
    }

    /// If we are a worker: checks if given peer is our manager,
    /// if we are a manager: checks if given peer is one of our workers.
    fn is_in_relationship_with(&self, peer_rc: PeerRc) -> bool {
        match &self.node_type_data {
            NodeTypeData::Manager(data) => {
                data.workers.get(&peer_rc.read().unwrap().peer_id).is_some()
            }
            NodeTypeData::Worker(data) => data.manager.as_ref().map_or(false, |(m, _, _)| {
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

    fn inject_network_behaviour_action(
        &mut self,
        evt: NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>,
    ) {
        self.events
            .push_front(InternalEvent::NetworkBehaviourAction(evt));
    }

    /// See [`send_to_peer_or_dial`] for information about internal functionning.
    fn inject_send_to_peer_or_dial_event(&mut self, peer_id: PeerId, event: HandlerIn<QueryId>) {
        let action = self.send_to_peer_or_dial(peer_id, event);
        self.inject_network_behaviour_action(action);
    }

    fn inject_behaviour_action(
        &mut self,
        event: NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>,
    ) {
        self.events
            .push_front(InternalEvent::NetworkBehaviourAction(event));
    }

    fn get_peer_or_insert(&mut self, peer_id: &PeerId) -> PeerRc {
        self.peers
            .entry(peer_id.clone())
            .or_insert_with(|| Arc::new(RwLock::new(Peer::new(peer_id.clone()))))
            .clone()
    }

    /// If the peer is connected, the message is pushed to a handler right away,
    /// otherwise it will be stored on hold and a dialing request will be made.
    /// If we are shutting down (i.e. [`is_shutting_down`] is `true`),
    /// the message won't be dropped.
    fn send_to_peer_or_dial(
        &mut self,
        peer_id: PeerId,
        event: HandlerIn<QueryId>,
    ) -> NetworkBehaviourAction<HandlerIn<QueryId>, EventOut> {
        if !self.is_shutting_down {
            let peer = self.get_peer_or_insert(&peer_id);
            let mut peer = peer.write().unwrap();

            if peer.connected {
                NetworkBehaviourAction::NotifyHandler {
                    handler: NotifyHandler::Any,
                    peer_id,
                    event,
                }
            } else {
                peer.pending_messages.push(event);
                NetworkBehaviourAction::DialPeer {
                    peer_id,
                    condition: DialPeerCondition::Disconnected,
                }
            }
        } else {
            NetworkBehaviourAction::GenerateEvent(EventOut::MsgDropped(peer_id, event))
        }
    }

    /// Send the event to the handler if we aren't shutting down
    /// (i.e. [`is_shutting_down`] is `true`).
    fn send_to_peer(
        &self,
        peer_id: PeerId,
        event: HandlerIn<QueryId>,
    ) -> NetworkBehaviourAction<HandlerIn<QueryId>, EventOut> {
        // TODO: let some messages pass ? (e.g. ManagerBye)
        if let HandlerIn::ManagerBye { .. } = event {
            NetworkBehaviourAction::NotifyHandler {
                handler: NotifyHandler::Any,
                peer_id,
                event,
            }
        } else if !self.is_shutting_down {
            NetworkBehaviourAction::NotifyHandler {
                handler: NotifyHandler::Any,
                peer_id,
                event,
            }
        } else {
            NetworkBehaviourAction::GenerateEvent(EventOut::MsgDropped(peer_id, event))
        }
    }

    pub fn handle_event_in(&mut self, event: EventIn) {
        let action = match event {
            EventIn::Ping => Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::Pong)),
            EventIn::Handler(peer_id, event) => {
                Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                    handler: NotifyHandler::Any,
                    peer_id,
                    event,
                })
            }
            EventIn::TasksExecute(peer_id, tasks) => {
                let event = HandlerIn::TasksExecute {
                    tasks,
                    user_data: self.next_query_unique_id(),
                };
                Poll::Ready(self.send_to_peer_or_dial(peer_id, event))
            }
            EventIn::TasksPing(peer_id, task_ids) => {
                let event = HandlerIn::TasksPing {
                    task_ids,
                    user_data: self.next_query_unique_id(),
                };
                Poll::Ready(self.send_to_peer_or_dial(peer_id, event))
            }
            EventIn::TasksPong {
                statuses,
                request_id,
            } => {
                if let NodeTypeData::Worker(WorkerData {
                    manager: Some((manager, _, _)),
                    ..
                }) = &self.node_type_data
                {
                    Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                        handler: NotifyHandler::Any,
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
            EventIn::TaskStatus(task_id, status) => {
                if let NodeTypeData::Worker(WorkerData {
                    manager: Some((manager, _, _)),
                    ..
                }) = &self.node_type_data
                {
                    let peer_id = manager.clone().read().unwrap().peer_id.clone();
                    let event = HandlerIn::TaskStatus {
                        task_id,
                        status,
                        user_data: self.next_query_unique_id(),
                    };
                    Poll::Ready(self.send_to_peer_or_dial(peer_id, event))
                } else {
                    Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::NoManager(
                        EventIn::TaskStatus(task_id, status),
                    )))
                }
            }
            EventIn::GetWorkers(oneshot) => {
                let w = if let NodeTypeData::Manager(data) = &self.node_type_data {
                    Some(data.workers.values().map(|(w, _, _)| w).cloned().collect())
                } else {
                    None
                };
                // TODO: expect
                oneshot
                    .send(w)
                    .expect("problem with sending answer through the oneshot.");

                Poll::Pending
            }
            // TODO: close the swarm and prevent any other in-connections
            // TODO: find a way to notify balthalib when we're done sending bye
            // TODO: special message ?
            // TODO: ExtendedSwarm::remove_listener ?
            EventIn::Bye => {
                let mut peer_ids = match &mut self.node_type_data {
                    NodeTypeData::Manager(ManagerData { workers, .. }) => {
                        workers.keys().cloned().collect()
                    }
                    NodeTypeData::Worker(WorkerData { manager, .. }) => {
                        if let Some((peer, _, _)) = manager.take() {
                            vec![peer.read().unwrap().peer_id.clone()]
                        } else {
                            Vec::new()
                        }
                    }
                };
                for id in peer_ids.drain(..) {
                    let user_data = self.next_query_unique_id();
                    self.inject_send_to_peer_or_dial_event(id, HandlerIn::ManagerBye { user_data });
                }
                self.is_shutting_down = true;
                Poll::Pending
            }
        };

        // If we're shutting down and trying to send a message, won't send it:
        let action =
            if let Poll::Ready(NetworkBehaviourAction::NotifyHandler { peer_id, event, .. }) =
                action
            {
                Poll::Ready(self.send_to_peer(peer_id, event))
            } else {
                action
            };

        if let Poll::Ready(action) = action {
            self.events
                .push_front(InternalEvent::NetworkBehaviourAction(action));
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

    fn inject_connected(&mut self, peer_id: &PeerId) {
        let peer = self.get_peer_or_insert(&peer_id);
        let mut peer = peer.write().unwrap();
        peer.connected = true;

        // If the node_type is unknown, plans to send a request:
        if peer.node_type.is_none() {
            self.events
                .push_front(InternalEvent::AskNodeType(peer_id.clone()));
        }

        peer.pending_messages.drain(..).for_each(|msg| {
            self.events
                .push_front(InternalEvent::SendMessage(peer_id.clone(), msg))
        });

        self.inject_generate_event(EventOut::PeerConnected(peer_id.clone()));
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId) {
        if let Some(peer) = self.peers.get_mut(peer_id) {
            {
                let mut peer = peer.write().unwrap();
                peer.connected = false;
            }
            self.inject_generate_event(EventOut::PeerDisconnected(peer_id.clone()));
        } else {
            panic!(
                "Peer `{:?}` doesn't exist, so can't be disconnected.",
                peer_id
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

    /*
    use libp2p::core::connection::ConnectedPoint;
    fn inject_connection_established(
        &mut self,
        peer_id: &PeerId,
        conn: &ConnectionId,
        point: &ConnectedPoint,
    ) {
        eprintln!("Connection est : {:?} {:?} {:?}", peer_id, conn, point);
    }
    fn inject_connection_closed(
        &mut self,
        peer_id: &PeerId,
        conn: &ConnectionId,
        point: &ConnectedPoint,
    ) {
        eprintln!("Connection closed : {:?} {:?} {:?}", peer_id, conn, point);
    }
    */

    /*
    fn inject_dial_failure(&mut self, peer_id: &PeerId) {
        eprintln!("ERR dial failure for : {:?}", peer_id);
    }
    */

    fn inject_listener_closed(&mut self, id: ListenerId, reason: Result<(), &std::io::Error>) {
        eprintln!("ERR listener closed {:?} : {:?}", id, reason);
    }

    fn inject_listener_error(&mut self, id: ListenerId, err: &(dyn error::Error + 'static)) {
        eprintln!("ERR listener {:?} : {:?}", id, err);
    }

    fn inject_event(
        &mut self,
        peer_id: PeerId,
        _connection: ConnectionId,
        event: <<Self::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::OutEvent,
    ) {
        self.inject_handler_event(peer_id, event);
    }

    // TODO: break this function into one or several external functions for clearer parsing ?
    // If yes, update the module doc.
    // TODO: too many PeerId clone and such ?
    fn poll(
        &mut self,
        cx: &mut Context,
        _params: &mut impl PollParameters
) -> Poll<NetworkBehaviourAction<<<Self::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::InEvent, Self::OutEvent>>{
        // Delays waking us up.
        {
            if let Poll::Ready(Some(_)) = self.delays.poll_expired(cx) {
                // eprintln!("Wake up Neo...");
            }
        }

        // Pinging manager or workers we're in relationship with.
        // Not necessary if we're shutting down.
        if !self.is_shutting_down {
            use CheckManagerWorkerRelationshipAction::*;
            let manager_check_interval = self.manager_check_interval;
            let manager_timeout = self.manager_timeout;
            let mut states = match &mut self.node_type_data {
                NodeTypeData::Manager(ManagerData { workers, .. }) => workers
                    .iter_mut()
                    .filter_map(|(peer_id, (_, last_ping, last_pong))| {
                        match check_manager_worker_relationship(
                            last_ping,
                            last_pong,
                            manager_check_interval,
                            manager_timeout,
                        ) {
                            Continuing => None,
                            state => Some((peer_id.clone(), state)),
                        }
                    })
                    .collect(),
                NodeTypeData::Worker(WorkerData {
                    manager: Some((manager_rc, last_ping, last_pong)),
                    ..
                }) => {
                    let manager_rc = manager_rc.clone();
                    let peer_id = manager_rc.read().unwrap().peer_id.clone();
                    let state = check_manager_worker_relationship(
                        last_ping,
                        last_pong,
                        manager_check_interval,
                        manager_timeout,
                    );
                    vec![(peer_id, state)]
                }
                // Find a manager here...
                _ => Vec::new(),
            };
            // TODO: cost of adding all here to the list instead of directly returning ?
            for (peer_id, state) in states.drain(..) {
                match state {
                    TimedOut => {
                        let user_data = self.next_query_unique_id();
                        self.inject_send_to_peer_or_dial_event(
                            peer_id.clone(),
                            HandlerIn::ManagerBye { user_data },
                        );

                        let evt = match break_worker_manager_relationship(&mut self.node_type_data, &peer_id) {
                            Some(NodeType::Manager) => EventOut::WorkerTimedOut(peer_id),
                            Some(NodeType::Worker) => EventOut::ManagerTimedOut(peer_id),
                            None => unreachable!("We shouldn't be here if we weren't in a worker/manager relationship."),
                        };
                        self.inject_generate_event(evt);
                    }
                    Ping => {
                        let user_data = self.next_query_unique_id();
                        self.delays.insert((), self.manager_timeout);
                        self.inject_send_to_peer_or_dial_event(
                            peer_id,
                            HandlerIn::ManagerPing { user_data },
                        );
                    }
                    Continuing => (),
                }
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
                        Poll::Ready(NetworkBehaviourAction::DialPeer {
                            peer_id,
                            condition: DialPeerCondition::Disconnected,
                        })
                    } else {
                        Poll::Pending
                    }
                }
                InternalEvent::SendMessage(peer_id, event) => {
                    Poll::Ready(self.send_to_peer_or_dial(peer_id, event))
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
                    Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                        handler: NotifyHandler::Any,
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
/// > a `Poll::Ready(NetworkBehaviourAction::NotifyHandler {..})` value.
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
        HandlerOut::NotMine { .. } => not_mine(&mut behaviour.node_type_data, peer_id),
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
        HandlerOut::ManagerBye { request_id } => {
            wrap_answer(peer_id.clone(), manager_bye(behaviour, peer_id, request_id))
        }
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
        HandlerOut::ManagerPong { user_data } => manager_pong(behaviour, peer_id, user_data),
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
                    tasks: Vec::new(),
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
