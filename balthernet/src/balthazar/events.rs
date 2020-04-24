use super::*;

/// Events injected into [`BalthBehaviour`] by the outside through the behaviour's channel.
#[derive(Debug)]
pub enum EventIn {
    Ping,
    /// Sending a message to the peer (new request or answer to one from the exterior).
    Handler(PeerId, HandlerIn<QueryId>),
    /// Asks worker `peer_id` to execute given task with given arguments.
    TasksExecute(PeerId, Vec<worker::TaskExecute>),
    /// Request statuses of given task ids, expects a [`EventOut::TasksPong`] in return.
    TasksPing(PeerId, Vec<TaskId>),
    /// Answer of a [`EventOut::TasksPing`] request.
    TasksPong {
        statuses: Vec<(TaskId, TaskStatus)>,
        request_id: RequestId,
    },
    /// We advertise a new status task for a given task_id.
    TaskStatus(TaskId, TaskStatus),
    /// Get current list of workers.
    GetWorkers(oneshot::Sender<Option<Vec<PeerRc>>>),
    /// The node will shut down... Send ManagerBye and all.
    Bye,
}

/// Event returned by [`BalthBehaviour`] towards the Swarm when polled.
#[derive(Debug)]
pub enum EventOut {
    /// Node type discovered for a peer.
    PeerHasNewType(PeerId, NodeType),
    /// Peer has been connected.
    PeerConnected(PeerId),
    /// Peer has been disconnected from all endpoints.
    PeerDisconnected(PeerId),
    /// Answer to a [`EventIn::Ping`].
    Pong,
    /// When a peer sends us a message, but we aren't in a worker-manager relationship.
    NotMine(PeerId, HandlerOut<QueryId>),
    /// Events created by [`Balthandler`] which are not handled directly in [`BalthBehaviour`]
    Handler(PeerId, HandlerOut<QueryId>),
    /// A new worker is now managed by us.
    // TODO: specs directly or by ref?
    WorkerNew(PeerId, WorkerSpecs),
    /// When in a relationship, on of our workers answered a ping request, so the
    /// relationship still holds.
    WorkerPong(PeerId),
    /// One of our workers hasn't answered a ping request in the set time out.
    // TODO: just return Bye to abstract the networking to the app ?
    WorkerTimedOut(PeerId),
    /// When a worker stops being managed by us.
    WorkerBye(PeerId),
    /// A manager has accepted acting as our manager, so we will receive orders from it now on.
    ManagerNew(PeerId),
    /// The manager we requested refused.
    ManagerRefused(PeerId),
    /// When in a relationship, our manager answered a ping request, so the
    /// relationship still holds.
    ManagerPong(PeerId),
    /// Our manager hasn't answered a ping request in the set time out.
    // TODO: just return Bye to abstract the networking to the app ?
    ManagerTimedOut(PeerId),
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
    /// This worker node has been requested by its manager to execute this list of tasks.
    TasksExecute(Vec<worker::TaskExecute>),
    /// Our manager asks about the status of given tasks, the `request_id` must be passed back to
    /// match the answer with the request.
    /// Expects a [`EventIn::TasksPong`] in return.
    TasksPing {
        task_ids: Vec<TaskId>,
        request_id: RequestId,
    },
    /// Answer to one of our [`EventIn::TasksPing`].
    TasksPong {
        peer_id: PeerId,
        statuses: Vec<(TaskId, TaskStatus)>,
    },
    /// Our manager asked us to stop working on these task ids.
    TasksAbord(Vec<TaskId>),
    /// One of our workers advertises a new status for task `task_id`.
    TaskStatus {
        peer_id: PeerId,
        task_id: TaskId,
        status: TaskStatus,
    },
    /// Cannot send message because we don't have any manager.
    NoManager(EventIn),
    /// Message won't be sent, prob because we're shutting down.
    MsgDropped(PeerId, HandlerIn<QueryId>),
}

#[derive(Debug)]
pub struct ManagerData {
    pub config: ManagerConfig,
    pub workers: HashMap<PeerId, (PeerRc, Instant, Instant)>,
}

#[derive(Debug)]
pub struct WorkerData {
    pub config: WorkerConfig,
    pub specs: WorkerSpecs,
    /// If this worker is in a worker-manager relationship, this stores a link to the manager, the
    /// instant of the last [`ManagerPing`](`worker::ManagerPing`) sent and
    /// the last time we've received a [`ManagerPong`](`worker::ManagerPong`) from it.
    pub manager: Option<(PeerRc, Instant, Instant)>,
}

pub type NodeTypeData = NodeTypeContainer<ManagerData, WorkerData>;

/// Peer data as used by [`BalthBehaviour`].
#[derive(Debug)]
pub struct Peer {
    pub peer_id: PeerId,
    /// Known addresses
    pub addrs: HashSet<Multiaddr>,
    /// Is peer connected?
    pub connected: bool,
    /// Has it already been dialed at least once?
    /// TODO: use a date to recheck periodically ?
    pub dialed: bool,
    /// Defines the node type if it is known.
    pub node_type: Option<NodeTypeContainer<(), Option<WorkerSpecs>>>,
    /// Messages waiting for a completed dial to be sent.
    pub pending_messages: Vec<HandlerIn<QueryId>>,
}

impl Peer {
    pub fn new(peer_id: PeerId) -> Self {
        Peer {
            peer_id,
            addrs: HashSet::new(),
            connected: false,
            dialed: false,
            node_type: None,
            pending_messages: Vec::new(),
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

// TODO: stop using behaviour directly ?
// TODO: tests

pub fn wrap_answer(
    peer_id: PeerId,
    event: HandlerIn<QueryId>,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    Poll::Ready(NetworkBehaviourAction::NotifyHandler {
        handler: NotifyHandler::Any,
        peer_id,
        event,
    })
}

/// Check if the peer is in relationship with us, if yes does the given action,
/// otherwise sends [`worker::NotMine`] to the peer.
pub fn needs_relationship_with<F, G>(
    behaviour: &mut BalthBehaviour,
    peer_rc: PeerRc,
    peer_id: PeerId,
    request_id: RequestId,
    action_if_in_relashionship: F,
    clone_event: G,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>>
where
    F: FnOnce(
        &mut BalthBehaviour,
        RequestId,
    ) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>>,
    G: FnOnce() -> HandlerOut<QueryId>,
{
    if behaviour.is_in_relationship_with(peer_rc) {
        action_if_in_relashionship(behaviour, request_id)
    } else {
        behaviour.inject_generate_event(EventOut::NotMine(peer_id.clone(), clone_event()));

        wrap_answer(peer_id, HandlerIn::NotMine { request_id })
    }
}

/// If the two nodes are in a relationship, breaks it and returns the kind of node we
/// are, otherwise does nothing and returns [`None`].
pub fn break_worker_manager_relationship(
    node_type_data: &mut NodeTypeData,
    peer_id: &PeerId,
) -> Option<NodeType> {
    match node_type_data {
        NodeTypeData::Worker(WorkerData { manager, .. }) => {
            if let Some((man, _, _)) = manager {
                if *peer_id == man.read().unwrap().peer_id {
                    *manager = None;
                    Some(NodeType::Worker)
                } else {
                    None
                }
            } else {
                None
            }
        }
        NodeTypeData::Manager(ManagerData { workers, .. }) => {
            if workers.remove(peer_id).is_some() {
                Some(NodeType::Manager)
            } else {
                None
            }
        }
    }
}

/// If [`BalthBehaviour::manager_timeout`] has elapsed after last [`ManagerPing`](`proto::worker::ManagerPing`), then we consider the peer as offline,
/// return [`TimedOut`](`CheckManagerWorkerRelationshipAction::TimedOut`) and the
/// worker/manager relationship should be broken.
/// If the last received [`ManagerPong`](`proto::worker::ManagerPong`) message was before [`BalthBehaviour::manager_check_interval`],
/// we reset the [`last_ping`] to now and return
/// [`Ping`](`CheckManagerWorkerRelationshipAction::Ping`) and a new [`ManagerPing`](`proto::worker::ManagerPing`) should be sent.
/// Otherwise, we return [`Continuing`](`CheckManagerWorkerRelationshipAction::Continuing`)
/// and no further action should be done.
pub fn check_manager_worker_relationship(
    last_ping: &mut Instant,
    last_pong: &Instant,
    manager_check_interval: Duration,
    manager_timeout: Duration,
) -> CheckManagerWorkerRelationshipAction {
    if *last_pong < *last_ping // 0, we haven't received any answer to the last ping
        && last_pong.elapsed() >= manager_timeout
    {
        CheckManagerWorkerRelationshipAction::TimedOut
    } else if last_pong.elapsed() > manager_check_interval
        && last_ping.elapsed() > manager_check_interval
    // && *last_pong >= *last_ping // 0, we received an answer to the last ping
    {
        *last_ping = Instant::now();
        CheckManagerWorkerRelationshipAction::Ping
    } else {
        CheckManagerWorkerRelationshipAction::Continuing
    }
}

pub enum CheckManagerWorkerRelationshipAction {
    TimedOut,
    Ping,
    Continuing,
}

/// A node has advertised its node type via [`worker::NodeTypeRequest`].
pub fn node_type_request(
    behaviour: &mut BalthBehaviour,
    peer_rc: PeerRc,
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
pub fn node_type_answer(
    behaviour: &mut BalthBehaviour,
    peer_rc: PeerRc,
    peer_id: PeerId,
    node_type: NodeType,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    let mut peer = peer_rc.write().unwrap();

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
                Some(data.specs.clone())
            } else {
                None
            }
        } else {
            None
        };

        peer.node_type = Some(node_type.into());
        behaviour.inject_generate_event(EventOut::PeerHasNewType(peer_id.clone(), node_type));

        if let Some(worker_specs) = request_man {
            Poll::Ready(NetworkBehaviourAction::NotifyHandler {
                handler: NotifyHandler::Any,
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
pub fn not_mine(
    node_type_data: &mut NodeTypeData,
    peer_id: PeerId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    match break_worker_manager_relationship(node_type_data, &peer_id) {
        Some(NodeType::Worker) => Poll::Ready(NetworkBehaviourAction::GenerateEvent(
            EventOut::ManagerBye(peer_id),
        )),
        Some(NodeType::Manager) => Poll::Ready(NetworkBehaviourAction::GenerateEvent(
            EventOut::WorkerBye(peer_id),
        )),
        None => Poll::Pending,
    }
}

/// We received a [`worker::ManagerRequest`].
pub fn manager_request(
    behaviour: &mut BalthBehaviour,
    peer_rc: PeerRc,
    peer_id: PeerId,
    worker_specs: WorkerSpecs,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    if let NodeTypeData::Manager(ref mut data) = behaviour.node_type_data {
        // TODO: more conditions for accepting workers ?
        // TODO: limit ?
        if let Some(NodeTypeContainer::Worker(ref mut specs_opt)) =
            peer_rc.write().unwrap().node_type
        {
            // TODO: what should be done if some specs are already known ?
            // TODO: avoid cloning ?
            *specs_opt = Some(worker_specs.clone());
            data.workers.insert(
                peer_id.clone(),
                (peer_rc.clone(), Instant::now(), Instant::now()),
            );
            behaviour.inject_generate_event(EventOut::WorkerNew(peer_id, worker_specs));
            HandlerIn::ManagerAnswer {
                accepted: true,
                request_id,
            }
        } else {
            behaviour.inject_generate_event(EventOut::MsgFromIncorrectNodeType {
                peer_id,
                known_type: peer_rc.read().unwrap().node_type_into(),
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
pub fn manager_answer(
    behaviour: &mut BalthBehaviour,
    peer_rc: PeerRc,
    peer_id: PeerId,
    accepted: bool,
    user_data: QueryId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    let peer_id_clone = peer_id.clone();
    let (evt, send_bye) = if let NodeTypeData::Worker(ref mut data) = behaviour.node_type_data {
        match (
            accepted,
            &data.manager,
            peer_rc.read().unwrap().node_type_into(),
        ) {
            (true, None, Some(NodeType::Manager)) => {
                if data.config.is_manager_authorized(
                    Some(&peer_id),
                    &peer_rc.read().unwrap().addrs_as_vec()[..],
                ) {
                    data.manager = Some((peer_rc.clone(), Instant::now(), Instant::now()));
                    behaviour
                        .delays
                        .insert((), behaviour.manager_check_interval);
                    (Some(EventOut::ManagerNew(peer_id)), false)
                } else {
                    (Some(EventOut::ManagerUnauthorized(peer_id)), true)
                }
            }
            (accepted, Some((manager, _, _)), Some(NodeType::Manager)) => {
                if manager.read().unwrap().peer_id == peer_rc.read().unwrap().peer_id {
                    let user_data = behaviour.next_query_unique_id();
                    behaviour.inject_send_to_peer_or_dial_event(
                        peer_id,
                        HandlerIn::ManagerPing { user_data },
                    );
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
                    known_type: peer_rc.read().unwrap().node_type_into(),
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

        Poll::Ready(NetworkBehaviourAction::NotifyHandler {
            handler: NotifyHandler::Any,
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
pub fn manager_bye(
    behaviour: &mut BalthBehaviour,
    peer_id: PeerId,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    let node_type = break_worker_manager_relationship(&mut behaviour.node_type_data, &peer_id);
    if let Some(node_type) = node_type {
        let evt = match node_type {
            NodeType::Worker => EventOut::ManagerBye(peer_id),
            NodeType::Manager => EventOut::WorkerBye(peer_id),
        };
        behaviour.inject_generate_event(evt);
    }

    HandlerIn::Ack { request_id }
}

/// We received a [`worker::ManagerPing`].
pub fn manager_ping(request_id: RequestId) -> HandlerIn<QueryId> {
    HandlerIn::ManagerPong { request_id }
}

// TODO: use ping as well to reset interval?
/// We received a [`worker::ManagerPong`].
pub fn manager_pong(
    behaviour: &mut BalthBehaviour,
    peer_id: PeerId,
    user_data: QueryId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    let evt = match &mut behaviour.node_type_data {
        // TODO: two gets?
        NodeTypeData::Manager(ManagerData { workers, .. })
            if workers
                .get(&peer_id)
                .map(|(peer_rc, last_ping, list_pong)| {
                    if peer_rc.read().unwrap().peer_id == peer_id {
                        Some((peer_rc, last_ping, list_pong))
                    } else {
                        None
                    }
                })
                .flatten()
                .is_some() =>
        {
            let (_, _, ref mut last_pong) = workers.get_mut(&peer_id).expect("Checked earlier.");
            *last_pong = Instant::now();

            EventOut::WorkerPong(peer_id)
        }
        NodeTypeData::Worker(WorkerData {
            manager: Some((manager_rc, _, last_pong)),
            ..
        }) if manager_rc.read().unwrap().peer_id == peer_id => {
            *last_pong = Instant::now();
            behaviour
                .delays
                .insert((), behaviour.manager_check_interval);
            EventOut::ManagerPong(peer_id)
        }
        _ => EventOut::NotMine(peer_id, HandlerOut::ManagerPong { user_data }),
    };

    Poll::Ready(NetworkBehaviourAction::GenerateEvent(evt))
}

/// We received a [`worker::TasksExecute`].
pub fn tasks_execute(
    behaviour: &mut BalthBehaviour,
    tasks: Vec<worker::TaskExecute>,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    behaviour.inject_generate_event(EventOut::TasksExecute(tasks));
    HandlerIn::Ack { request_id }
}

/// We received a [`worker::TasksPing`].
pub fn tasks_ping(
    task_ids: Vec<TaskId>,
    request_id: RequestId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    Poll::Ready(NetworkBehaviourAction::GenerateEvent(EventOut::TasksPing {
        task_ids,
        request_id,
    }))
}

/// We received a [`worker::TasksPong`].
pub fn tasks_pong(
    behaviour: &mut BalthBehaviour,
    peer_rc: PeerRc,
    peer_id: PeerId,
    statuses: Vec<(TaskId, TaskStatus)>,
    user_data: QueryId,
) -> Poll<NetworkBehaviourAction<HandlerIn<QueryId>, EventOut>> {
    let evt = if behaviour.is_in_relationship_with(peer_rc) {
        EventOut::TasksPong { peer_id, statuses }
    } else {
        EventOut::NotMine(
            peer_id,
            HandlerOut::TasksPong {
                statuses,
                user_data,
            },
        )
    };

    Poll::Ready(NetworkBehaviourAction::GenerateEvent(evt))
}

/// We received a [`worker::TasksAbord`].
pub fn tasks_abord(
    behaviour: &mut BalthBehaviour,
    task_ids: Vec<TaskId>,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    behaviour.inject_generate_event(EventOut::TasksAbord(task_ids));

    HandlerIn::Ack { request_id }
}

/// We received a [`worker::TaskStatus`].
pub fn task_status(
    behaviour: &mut BalthBehaviour,
    peer_id: PeerId,
    task_id: TaskId,
    status: TaskStatus,
    request_id: RequestId,
) -> HandlerIn<QueryId> {
    behaviour.inject_generate_event(EventOut::TaskStatus {
        peer_id,
        task_id,
        status,
    });

    HandlerIn::Ack { request_id }
}
