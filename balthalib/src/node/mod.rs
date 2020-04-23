//! Grouping module for all balthazar sub-modules.

extern crate async_ctrlc;

use futures::{
    channel::mpsc::{channel, Sender},
    future, join, select, FutureExt, SinkExt, StreamExt,
};
use std::{
    borrow::Cow,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{runtime::Runtime, sync::RwLock};

use async_ctrlc::CtrlC;
use chain::Chain;
use misc::{
    job::{Address, DefaultHash, ProgramKind, TaskId},
    multihash::Keccak256,
    shared_state::{PeerId, SharedState},
    WorkerSpecs,
};
use proto::{
    manager as man,
    worker::{self, TaskErrorKind, TaskExecute},
    NodeType, TaskStatus,
};
use run::{Executor, WasmExecutor};
use store::{FetchStorage, StoragesWrapper};

use super::{BalthazarConfig, Error};
mod shared_state;
use shared_state::Event as SharedStateEvent;
mod workers;
use workers::*;

const CHANNEL_SIZE: usize = 1024;

pub fn run(config: BalthazarConfig) -> Result<(), Error> {
    Runtime::new().unwrap().block_on(Balthazar::run(config))
}

/*
// TODO: cleaner and in self module
async fn get_keypair(keyfile_path: &Path) -> Result<Keypair, Error> {
    let mut bytes = fs::read(keyfile_path)
        .await
        .map_err(Error::KeyPairReadFileError)?;
    Keypair::rsa_from_pkcs8(&mut bytes).map_err(Error::KeyPairDecodingError)
}
*/

/// Internal events for Balthazar.
#[derive(Debug)]
enum Event {
    SharedStateProposal(man::Proposal),
    SharedStateChange(TaskId, shared_state::Event),
    // ChainJobs(chain::JobsEvent),
    // Swarm(net::EventOut),
    Error(Error),
    Log { kind: LogKind, msg: String },
}

impl fmt::Display for Event {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Log { kind, msg } => write!(fmt, "{} --- {}", kind, msg),
            _ => write!(fmt, "{:?}", self),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LogKind {
    Swarm,
    Worker,
    Manager,
    Blockchain,
    SharedState,
    // Error,
}

impl fmt::Display for LogKind {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let letter = match self {
            LogKind::Swarm => "S",
            LogKind::Worker => "W",
            LogKind::Manager => "M",
            LogKind::Blockchain => "B",
            LogKind::SharedState => "T",
            // LogKind::Error => "E",
        };
        write!(fmt, "{}", letter)
    }
}

#[derive(Clone)]
#[allow(clippy::type_complexity)]
struct Balthazar {
    // TODO: reference?
    peer_id: PeerId,
    // TODO: reference?
    config: Arc<BalthazarConfig>,
    inner_in: Sender<Event>,
    swarm_in: Sender<net::EventIn>,
    runner_in: Sender<worker::TaskExecute>,
    shared_state: Arc<RwLock<SharedState>>,
    // TODO: Avoid creating that when not used?
    workers: Arc<RwLock<Workers>>,
    // keypair: balthernet::identity::Keypair,
}

impl Balthazar {
    fn new(
        peer_id: PeerId,
        config: BalthazarConfig,
        inner_in: Sender<Event>,
        swarm_in: Sender<net::EventIn>,
        runner_in: Sender<worker::TaskExecute>,
    ) -> Self {
        Balthazar {
            peer_id,
            config: Arc::new(config),
            inner_in,
            swarm_in,
            runner_in,
            shared_state: Default::default(),
            workers: Default::default(),
        }
    }

    // TODO: don't re-create it, it opens a new connection each time...
    fn chain(&self) -> Chain {
        Chain::new(self.config.chain())
    }

    fn ethereum_address(&self) -> Result<&misc::job::Address, chain::Error> {
        self.config.chain().ethereum_address()
            .as_ref()
            .ok_or(chain::Error::MissingLocalAddress)
    }

    async fn send_msg_to_behaviour(&self, event: net::EventIn) {
        if let Err(e) = self.swarm_in.clone().send(event).await {
            panic!("Swarm channel error: {:?}", Error::SwarmChannelError(e));
        }
    }

    pub async fn run(config: BalthazarConfig) -> Result<(), Error> {
        let node_type = *config.node_type();
        println!("Starting as {:?}...", node_type);

        let specs = WorkerSpecs::default();
        let keypair = balthernet::identity::Keypair::generate_secp256k1();

        let (swarm_in, swarm_out) = net::get_swarm(keypair.clone(), config.net(), Some(&specs));
        let (inner_in, inner_out) = channel(CHANNEL_SIZE);
        let (runner_in, runner_out) = channel(CHANNEL_SIZE);

        let peer_id = keypair.public().into_peer_id();
        let balth = Balthazar::new(peer_id, config, inner_in, swarm_in, runner_in);

        // TODO: concurrent ?
        // TODO: looks dirty, is it ?
        let mut stream = Box::pin(
            inner_out
                .then(|e| balth.clone().handle_event(e))
                .skip_while(|r| futures::future::ready(r.is_ok())),
        );
        let channel_fut = stream.next();

        let ctrlc_fut = balth.clone().handle_ctrlc();
        // TODO: concurrent ?
        let swarm_fut = swarm_out.for_each(|e| balth.clone().handle_swarm_event(e));
        // TODO: actually use concurrent?
        let runner_fut = runner_out.for_each_concurrent(None, |t| balth.handle_runner(t));

        if let NodeType::Manager = node_type {
            let chain_fut = balth.handle_chain();
            select! {
                res = chain_fut.fuse() => res?,
                _ = swarm_fut.fuse() => (),
                _ = runner_fut.fuse() => (),
                res = channel_fut.fuse() => res.expect("Channel stream ended but there was no error.")?,
                _ = ctrlc_fut.fuse() => (),
            }
        } else {
            select! {
                _ = swarm_fut.fuse() => (),
                _ = runner_fut.fuse() => (),
                res = channel_fut.fuse() => res.expect("Channel stream ended but there was no error.")?,
                _ = ctrlc_fut.fuse() => (),
            }
        }

        eprintln!("Bye...");
        Ok(())
    }

    // TODO: ref as mutable ? or just clone tx ?
    async fn spawn_event(&self, event: Event) {
        let mut tx = self.inner_in.clone();
        if let Err(e) = tx.send(event).await {
            panic!("{:?}", Error::EventChannelError(e));
        }
    }

    async fn spawn_shared_state_change(&self, task_id: TaskId, event: SharedStateEvent) {
        self.spawn_event(Event::SharedStateChange(task_id, event))
            .await;
    }

    async fn spawn_log(&self, kind: LogKind, msg: String) {
        self.spawn_event(Event::Log { kind, msg }).await
    }

    /// Handle all inner events.
    async fn handle_event(self, event: Event) -> Result<(), Error> {
        match event {
            Event::SharedStateProposal(p) => {
                eprintln!("Event::SharedStateProposal");
                self.check_and_apply_proposal(p).await},
            Event::SharedStateChange(task_id, event) => {
                eprintln!("Event::SharedStateChange");
                self.handle_shared_state_change(task_id, event).await?
            }
            // Event::Swarm(e) => self.handle_swarm_event(e).await,
            // Event::ChainJobs(e) => self.handle_chain_event(e).await,
            Event::Log { .. } => {
                eprintln!("Event::Log");
                eprintln!("{}", event)
            },
            _ => unimplemented!(),
        }

        if let NodeType::Manager = self.config.node_type() {
                eprintln!("propose_assignements");
            self.propose_assignements().await?;
        }
                eprintln!("Done");
        Ok(())
    }

    /// Handle events coming out of smart-contracts.
    async fn handle_chain(&self) -> Result<(), Error> {
        let chain = self.chain();
        let addr = self.ethereum_address()?;
        chain
            .jobs_subscribe()
            .await?
            .for_each(|e| async {
                match e {
                    Ok(evt) => self.clone().handle_chain_event(evt, addr).await,
                    Err(e) => self.spawn_event(Event::Error(e.into())).await,
                }
            })
            .await;

        Ok(())
    }

    /// Handle events coming out of Swarm.
    // TODO: update changes in the tasks in the SC which didn't come from us.
    // We only react to TaskPending and really expect the SC doesn't change otherwise...
    async fn handle_chain_event(&self, event: chain::JobsEvent, ethereum_address: &Address) {
        self.spawn_log(LogKind::Blockchain, format!("{}", event))
            .await;
        if let chain::JobsEvent::TaskPending { task_id } = event {
            let msg = man::Proposal {
                task_id: task_id.into_bytes(),
                payment_address: Vec::from(ethereum_address.as_bytes()),
                proposal: Some(man::proposal::Proposal::NewTask(man::ProposeNewTask {})),
            };
            self.spawn_event(Event::SharedStateProposal(msg)).await;
        }
    }

    /// Handle Interruption event when Ctrl+C is pressed.
    async fn handle_ctrlc(self) {
        // TODO: dirty, make it an actual stream...
        let mut ctrlc = CtrlC::new().expect("cannot create Ctrl+C handler?");
        {
            future::poll_fn(|ctx| CtrlC::poll(Pin::new(&mut ctrlc), ctx)).await;
            eprintln!("Ctrl+C pressed, breaking relationships... :'(");

            self.send_msg_to_behaviour(net::EventIn::Bye).await;
        }
        {
            future::poll_fn(|ctx| CtrlC::poll(Pin::new(&mut ctrlc), ctx)).await;
            eprintln!("Ctrl+C pressed a second time, definetely quitting...");
        }
    }

    /// Delete known worker and if it were assigned, announce it unassigned.
    async fn delete_worker(&self, peer_id: PeerId) {
        let mut workers = self.workers.write().await;
        if let Some(mut assignments) = workers.remove(&peer_id) {
            let shared_state = self.shared_state.read().await;
            // Declare any assigned tasks it was doing as Aborted.
            for task_id in assignments.drain(..).filter_map(|a| {
                if let WorkerAssignment::Assigned(task_id) = a {
                    Some(task_id)
                } else {
                    None
                }
            }) {
                self.send_abord_proposal(&shared_state, &peer_id, &task_id)
                    .await;
            }
        } else {
            panic!("Unknown worker.");
        }
    }

    /// Handle a new task status from a worker.
    async fn handle_task_status(&self, peer_id: PeerId, task_id: TaskId, status: TaskStatus) {
        match status {
            TaskStatus::Completed(_) | TaskStatus::Error(_) => (),
            _ => return, // No need for handling for now.
        }

        let mut workers = self.workers.write().await;
        if !workers.unassign_slot(&peer_id, &task_id) {
            // TODO: not panic in case the worker sends a bad message...
            panic!("Unknown or unassigned worker.");
        }

        let ethereum_address = self.ethereum_address().unwrap();

        let proposal = match status {
            TaskStatus::Completed(_) => man::proposal::Proposal::Completed(man::ProposeCompleted {
                completion_signals_senders: Vec::new(),
                completion_signals: Vec::new(),
                selected_result: Some(man::ManTaskStatus {
                    task_id: task_id.clone().into_bytes(),
                    worker: peer_id.into_bytes(),
                    status: Some(worker::TaskStatus {
                        task_id: task_id.clone().into_bytes(),
                        status_data: status.into(),
                    }),
                }),
            }),
            TaskStatus::Error(reason) => {
                let shared_state = self.shared_state.read().await;

                let task = shared_state.tasks.get(&task_id).expect("Unknown task.");
                let nb_failures_diff = {
                    use TaskErrorKind::*;

                    match reason {
                        Aborted | Unknown => 0,
                        TimedOut | Download | Runtime => 1,
                    }
                };

                man::proposal::Proposal::Failure(man::ProposeFailure {
                    new_nb_failures: task.nb_failures() + nb_failures_diff,
                    kind: Some(man::propose_failure::Kind::Worker(
                        man::ProposeFailureWorker {
                            original_message_sender: Vec::new(),
                            original_message: Some(man::ManTaskStatus {
                                task_id: task_id.clone().into_bytes(),
                                worker: peer_id.into_bytes(),
                                status: Some(worker::TaskStatus {
                                    task_id: task_id.clone().into_bytes(),
                                    status_data: status.into(),
                                }),
                            }),
                        },
                    )),
                })
            }
            _ => unreachable!(),
        };

        let msg = man::Proposal {
            task_id: task_id.into_bytes(),
            payment_address: Vec::from(ethereum_address.as_bytes()),
            proposal: Some(proposal),
        };
        self.spawn_event(Event::SharedStateProposal(msg)).await;
    }

    /// Handle events coming out of Swarm.
    ///
    /// > **Note:** Each action here should be very quick, otherwise it the whole swarm will pause.
    async fn handle_swarm_event(self, event: net::EventOut) {
        match (self.config.node_type(), event) {
            (NodeType::Manager, net::EventOut::WorkerNew(peer_id)) => {
                {
                    let mut workers = self.workers.write().await;
                    // TODO: use workers specs.
                    workers.push(peer_id.clone(), 1);
                }
                self.spawn_log(
                    LogKind::Swarm,
                    format!("event: {:?}", net::EventOut::WorkerNew(peer_id.clone())),
                )
                .await;
                if let Some((wasm, args)) = self.config.wasm() {
                    self.send_manual_task(peer_id, wasm.clone(), args).await;
                }
            }
            (NodeType::Manager, net::EventOut::WorkerBye(peer_id))
            | (NodeType::Manager, net::EventOut::WorkerTimedOut(peer_id)) => {
                self.spawn_log(
                    LogKind::Swarm,
                    format!("event: {:?}", net::EventOut::WorkerBye(peer_id.clone())),
                )
                .await;
                self.delete_worker(peer_id).await;
            }
            (
                NodeType::Manager,
                net::EventOut::TaskStatus {
                    peer_id,
                    task_id,
                    status,
                },
            ) => {
                eprintln!("41");
                self.spawn_log(
                    LogKind::Manager,
                    format!(
                        "Task status from peer `{}` for task `{}`: {}",
                        peer_id, task_id, status
                    ),
                )
                .await;
                eprintln!("4");
                self.handle_task_status(peer_id, task_id, status).await;
                eprintln!("5");
            }
            (NodeType::Worker, net::EventOut::TasksExecute(mut tasks)) => {
                let mut runner_in = self.runner_in.clone();
                for task in tasks.drain(..) {
                    // TODO: or just spawn a new task for it?
                    runner_in.send(task).await.expect("Runner channel closed?");
                }
            }
            // Muting those from the logging
            (_, net::EventOut::PeerConnected(_))
            | (_, net::EventOut::PeerDisconnected(_))
            | (_, net::EventOut::WorkerPong(_))
            | (_, net::EventOut::ManagerPong(_)) => (),
            (_, event) => {
                self.spawn_log(LogKind::Swarm, format!("event: {:?}", event))
                    .await;
            }
        }
    }

    async fn handle_shared_state_change(
        &self,
        task_id: TaskId,
        event: SharedStateEvent,
    ) -> Result<(), Error> {
        match event {
            SharedStateEvent::Pending => (),
            SharedStateEvent::Assigned { worker } => {
                let (shared_state, mut workers) =
                    join!(self.shared_state.read(), self.workers.write());
                if !workers.assign_slot(&worker, task_id.clone()) {
                    self.send_abord_proposal(&shared_state, &worker, &task_id)
                        .await;
                } else {
                    let job = shared_state.get_job_from_task_id(&task_id).unwrap();
                    let argument = {
                        let task = shared_state.tasks.get(&task_id).unwrap();
                        job.arguments()[task.arg_id() as usize].to_vec()
                    };
                    self.send_msg_to_behaviour(net::EventIn::TasksExecute(
                        worker,
                        vec![TaskExecute {
                            task_id: task_id.into_bytes(),
                            program_addresses: job.program_addresses().to_vec(),
                            program_hash: job.program_hash().clone().into(),
                            program_kind: job.program_kind().clone().into(),
                            argument,
                            timeout: job.timeout(),
                            max_network_usage: job.max_network_usage(),
                        }],
                    ))
                    .await;
                }
            }
            SharedStateEvent::Unassigned {
                worker,
                workers_manager: _,
            } => {
                let mut workers = self.workers.write().await;
                workers.unassign_slot(&worker, &task_id);
            }
        }
        Ok(())
    }

    async fn handle_runner(&self, task: TaskExecute) {
        // TODO: expect
        let task_id = TaskId::from_bytes(task.task_id).expect("not a correct multihash");
        self.send_msg_to_behaviour(net::EventIn::TaskStatus(
            task_id.clone(),
            TaskStatus::Pending,
        ))
        .await;
        let storage = StoragesWrapper::default();
        let string_program_address = &task.program_addresses[0][..];
        let string_argument = String::from_utf8_lossy(&task.argument[..]);

        self.spawn_log(
            LogKind::Worker,
            format!("will get program `{}`...", string_program_address),
        )
        .await;
        match storage
            .fetch(
                &task.program_addresses[0][..],
                storage
                    .get_size(&task.program_addresses[0][..])
                    .await
                    .unwrap(),
            )
            .await
        {
            Ok(wasm) => {
                self.spawn_log(
                    LogKind::Worker,
                    format!("received program `{}`.", string_program_address),
                )
                .await;
                self.spawn_log(
                    LogKind::Worker,
                    format!(
                        "spawning wasm executor for `{}` with argument `{}`...",
                        string_program_address, string_argument,
                    ),
                )
                .await;

                self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                    task_id.clone(),
                    TaskStatus::Started(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    ),
                ))
                .await;

                match WasmExecutor::default()
                    .run(
                        &wasm[..],
                        &task.argument[..],
                        task.timeout,
                        task.max_network_usage,
                    )
                    .0
                    .await
                {
                    Ok(result) => {
                        self.spawn_log(
                            LogKind::Worker,
                            format!(
                                "task result for `{}` with `{}`: `{:?}`",
                                string_program_address,
                                string_argument,
                                String::from_utf8_lossy(&result[..])
                            ),
                        )
                        .await;
                        self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                            task_id.clone(),
                            TaskStatus::Completed(result),
                        ))
                        .await;
                    }
                    Err(error) => {
                        self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                            task_id.clone(),
                            TaskStatus::Error(TaskErrorKind::Runtime),
                        ))
                        .await;
                        self.spawn_log(
                            LogKind::Worker,
                            format!(
                                "task error for `{}` with `{}`: `{:?}`",
                                string_program_address, string_argument, error
                            ),
                        )
                        .await;
                    }
                }
            }
            Err(error) => {
                self.send_msg_to_behaviour(net::EventIn::TaskStatus(
                    task_id.clone(),
                    TaskStatus::Error(TaskErrorKind::Download),
                ))
                .await;
                self.spawn_log(
                    LogKind::Worker,
                    format!(
                        "error while fetching `{}`: `{:?}`",
                        string_program_address, error
                    ),
                )
                .await;
            }
        }
    }

    async fn send_manual_task(&self, peer_id: PeerId, wasm: String, args: &[Vec<u8>]) {
        let storage = StoragesWrapper::default();
        let program_data = storage.fetch(&wasm[..], 1_000_000).await.unwrap();
        let program_hash = DefaultHash::digest(&program_data[..]).into_bytes();
        let tasks = args
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, argument)| TaskExecute {
                task_id: Keccak256::digest(&i.to_be_bytes()[..]).into_bytes(),
                program_addresses: vec![wasm.clone()],
                program_hash: program_hash.clone(),
                program_kind: ProgramKind::Wasm0m1n0.into(),
                argument,
                timeout: 100,
                max_network_usage: 100,
            })
            .collect();
        let args_str: Vec<Cow<str>> = args
            .iter()
            .map(|a| String::from_utf8_lossy(&a[..]))
            .collect();
        self.spawn_log(
            LogKind::Manager,
            format!(
                "Sending task `{}` with parameters {:?} to worker `{}`",
                wasm, args_str, peer_id
            ),
        )
        .await;
        self.send_msg_to_behaviour(net::EventIn::TasksExecute(peer_id, tasks))
            .await
    }

    async fn send_abord_proposal(
        &self,
        shared_state: &impl std::ops::Deref<Target = SharedState>,
        peer_id: &PeerId,
        task_id: &TaskId,
    ) {
        let ethereum_address = self.ethereum_address().unwrap();

        let task = shared_state.tasks.get(&task_id).expect("Unknown task.");

        let msg = man::Proposal {
            task_id: task_id.clone().into_bytes(),
            payment_address: Vec::from(ethereum_address.as_bytes()),
            proposal: Some(man::proposal::Proposal::Failure(man::ProposeFailure {
                new_nb_failures: task.nb_failures(),
                kind: Some(man::propose_failure::Kind::Worker(
                    man::ProposeFailureWorker {
                        original_message_sender: Vec::new(),
                        original_message: Some(man::ManTaskStatus {
                            task_id: task_id.clone().into_bytes(),
                            worker: peer_id.clone().into_bytes(),
                            status: Some(worker::TaskStatus {
                                task_id: task_id.clone().into_bytes(),
                                status_data: Some(worker::task_status::StatusData::Error(
                                    worker::TaskErrorKind::Aborted.into(),
                                )),
                            }),
                        }),
                    },
                )),
            })),
        };
        self.spawn_event(Event::SharedStateProposal(msg)).await;
    }

    // TODO: avoid creating too many proposals...
    // TODO: find a way to know when a pending can go back to available...
    async fn propose_assignements(&self) -> Result<(), Error> {
        eprintln!("0");
        let ethereum_address = self.ethereum_address()?;

        eprintln!("1");
        let (shared_state, mut workers) = join!(self.shared_state.read(), self.workers.write());
        eprintln!("2");
        // eprintln!("{:?}", shared_state.get_nb_unassigned_per_task());
        if !workers.get_unassigned_workers().is_empty() {
        eprintln!("3");
            for (task_id, nb_unassigned) in shared_state.get_nb_unassigned_per_task().drain(..) {
        eprintln!("4");
                let unassigned_workers = workers.get_unassigned_workers_sorted();
                // eprintln!("{:?}", unassigned_workers);
                if !unassigned_workers.is_empty() {
        eprintln!("5");
                    // cloning is needed because each `unassigned_workers` is immutable ref,
                    // but `workers.reserve_slot` requires a mutable one.
                    // TODO: avoid cloning...
                    let unassigned_workers: Vec<_> = unassigned_workers
                        .iter()
                        .take(nb_unassigned)
                        .map(|w| (*w).clone())
                        .collect();
                    for w in unassigned_workers.iter() {
        eprintln!("6");
                        workers.reserve_slot(w);
                    }

                    let selected_offers: Vec<_> = unassigned_workers
                        .iter()
                        .take(nb_unassigned)
                        .map(|peer_id| {
                            let mut offer = man::Offer::default();
                            offer.task_id = task_id.clone().into_bytes();
                            offer.worker = (*peer_id).clone().into_bytes();
                            // TODO: our address
                            offer.workers_manager = self.peer_id.clone().into_bytes();
                            offer.payment_address = Vec::from(ethereum_address.as_bytes());
                            offer.worker_price = 1;
                            offer.network_price = 1;
                            offer
                        })
                        .collect();
        eprintln!("7");

                    // TODO: check workers specs, for now we expect them all to have the same...
                    let msg = man::Proposal {
                        task_id: task_id.clone().into_bytes(),
                        payment_address: Vec::from(ethereum_address.as_bytes()),
                        proposal: Some(man::proposal::Proposal::Scheduling(
                            man::ProposeScheduling {
                                all_offers_senders: Vec::new(),
                                all_offers: Vec::new(),
                                selected_offers,
                            },
                        )),
                    };
        eprintln!("8");
                    self.spawn_event(Event::SharedStateProposal(msg)).await;
        eprintln!("9");
                }
            }
        }
        Ok(())
    }
}
