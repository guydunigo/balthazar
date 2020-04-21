//! Grouping module for all balthazar sub-modules.
// TODO: remove allws
// #![allow(unused_imports)]
// #![allow(dead_code)]

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
    config: Arc<BalthazarConfig>,
    inner_tx: Sender<Event>,
    swarm_in: Sender<net::EventIn>,
    shared_state: Arc<RwLock<SharedState>>,
    // TODO: Avoid creating that when not used?
    workers: Arc<RwLock<Vec<(PeerId, Option<TaskId>)>>>,
    // keypair: balthernet::identity::Keypair,
}

impl Balthazar {
    fn new(
        config: BalthazarConfig,
        inner_tx: Sender<Event>,
        swarm_in: Sender<net::EventIn>,
    ) -> Self {
        Balthazar {
            config: Arc::new(config),
            inner_tx,
            swarm_in,
            shared_state: Arc::new(RwLock::new(SharedState::default())),
            workers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    // TODO: re-create it each time ?
    fn chain(&self) -> Chain {
        Chain::new(self.config.chain())
    }

    /*
    /// Returns a worker who isn't computing anything.
    /// Returns `None` if we are not a manager.
    // TODO: race condition between returning PeerRc and next lock: peer gets busy again
    async fn find_available_workers<'a>(&'a self) -> (impl 'a + std::ops::DerefMut<Target = VecDeque<(JobId, TaskId, Option<PeerRc>)>>,impl Iterator<Item=PeerRc>) {
        let pending_tasks = self.pending_tasks().await;
        let busy_peers_rc: Vec<_> = pending_tasks
            .iter()
            .filter_map(|(_, _, o)| if let Some(p) = o {
                Some(p)
            } else { None })
            .collect();
        let mut busy_peers = Vec::new();
        for p in busy_peers_rc.iter() {
            busy_peers.push(p.read().expect("couldn't lock on peer").peer_id.clone());
        }

        let (tx, rx) = oneshot::channel();
        self.send_msg_to_behaviour(net::EventIn::GetWorkers(tx))
            .await;
        // TODO: expect
        let workers = rx.await
            .expect("Other end dropped without answer.")?;

        let mut iter= Vec::new();
        for w in workers.iter() {
            if busy_peers.contains(&w.read().expect("couldn't lock on peer").peer_id) {
                iter.push(w.clone());
            }
        }

        iter.drain(..)
    }
    */

    async fn send_msg_to_behaviour(&self, event: net::EventIn) {
        if let Err(e) = self.swarm_in.clone().send(event).await {
            panic!("Swarm channel error: {:?}", Error::SwarmChannelError(e));
        }
    }

    pub async fn run(config: BalthazarConfig) -> Result<(), Error> {
        let node_type = *config.node_type();
        println!("Starting as {:?}...", node_type);

        let (inner_tx, inner_rx) = channel(CHANNEL_SIZE);

        let specs = WorkerSpecs::default();
        let keypair = balthernet::identity::Keypair::generate_secp256k1();
        let (swarm_in, swarm_out) = net::get_swarm(keypair.clone(), config.net(), Some(&specs));

        let balth = Balthazar::new(config, inner_tx, swarm_in);

        // TODO: concurrent ?
        // TODO: looks dirty, is it ?
        let mut stream = Box::pin(
            inner_rx
                .then(|e| balth.clone().handle_event(e))
                .skip_while(|r| futures::future::ready(r.is_ok())),
        );
        let channel_fut = stream.next();

        let ctrlc_fut = balth.clone().handle_ctrlc();
        let swarm_fut = swarm_out.for_each(|e| balth.clone().handle_swarm_event(e));

        if let NodeType::Manager = node_type {
            let chain_fut = balth.clone().handle_chain();
            select! {
                res = chain_fut.fuse() => res?,
                _ = swarm_fut.fuse() => (),
                res = channel_fut.fuse() => res.expect("Channel stream ended but there was no error.")?,
                _ = ctrlc_fut.fuse() => (),
            }
        } else {
            select! {
                _ = swarm_fut.fuse() => (),
                res = channel_fut.fuse() => res.expect("Channel stream ended but there was no error.")?,
                _ = ctrlc_fut.fuse() => (),
            }
        }

        eprintln!("Bye...");
        Ok(())
    }

    // TODO: ref as mutable ? or just clone tx ?
    async fn spawn_event(&self, event: Event) {
        let mut tx = self.inner_tx.clone();
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
            Event::SharedStateProposal(p) => self.check_and_apply_proposal(p).await,
            Event::SharedStateChange(task_id, event) => {
                self.handle_shared_state_change(task_id, event).await?
            }
            // Event::Swarm(e) => self.handle_swarm_event(e).await,
            // Event::ChainJobs(e) => self.handle_chain_event(e).await,
            Event::Log { .. } => eprintln!("{}", event),
            _ => unimplemented!(),
        }
        Ok(())
    }

    /// Handle events coming out of smart-contracts.
    async fn handle_chain(self) -> Result<(), Error> {
        let chain = self.chain();
        let addr = chain.local_address()?;
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
    async fn handle_chain_event(&self, event: chain::JobsEvent, local_address: &Address) {
        self.spawn_log(LogKind::Blockchain, format!("{}", event))
            .await;
        if let chain::JobsEvent::TaskPending { task_id } = event {
            let msg = man::Proposal {
                task_id: task_id.into_bytes(),
                payment_address: Vec::from(local_address.as_bytes()),
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

    /// Handle events coming out of Swarm.
    async fn handle_swarm_event(self, event: net::EventOut) {
        match (self.config.node_type(), event) {
            (NodeType::Manager, net::EventOut::WorkerNew(peer_id)) => {
                {
                    let mut workers = self.workers.write().await;
                    workers.push((peer_id.clone(), None));
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
            (NodeType::Manager, net::EventOut::WorkerBye(peer_id)) => {
                {
                    let mut workers = self.workers.write().await;
                    if let Some((id, _)) = workers
                        .iter()
                        .enumerate()
                        .find(|(_, (worker_id, _))| *worker_id == peer_id)
                    {
                        workers.remove(id);
                    }
                }
                self.spawn_log(
                    LogKind::Swarm,
                    format!("event: {:?}", net::EventOut::WorkerBye(peer_id.clone())),
                )
                .await;
            }
            (
                NodeType::Manager,
                net::EventOut::TaskStatus {
                    peer_id,
                    task_id,
                    status,
                },
            ) => {
                self.spawn_log(
                    LogKind::Manager,
                    format!(
                        "Task status from peer `{}` for task `{}`: {}",
                        peer_id, task_id, status
                    ),
                )
                .await;
                if let TaskStatus::Completed(_) = status {
                    let mut workers = self.workers.write().await;
                    workers.iter_mut()
                        .find_map(|(w,p)| if *w == peer_id { Some(p) } else { None })
                        .expect("Worker unknown.").take().take();
                    let chain = self.chain();
                    let local_address = chain.local_address().unwrap();
                    let msg = man::Proposal {
                        task_id: task_id.clone().into_bytes(),
                        payment_address: Vec::from(local_address.as_bytes()),
                        proposal: Some(man::proposal::Proposal::Completed(man::ProposeCompleted {
                            completion_signals_senders: Vec::new(),
                            completion_signals: Vec::new(),
                            selected_result: Some(man::ManTaskStatus {
                                task_id: task_id.clone().into_bytes(),
                                worker: peer_id.into_bytes(),
                                status: Some(worker::TaskStatus {
                                    task_id: task_id.into_bytes(),
                                    status_data: status.into(),
                                }),
                            }),
                        })),
                    };
                    self.spawn_event(Event::SharedStateProposal(msg)).await;
                }
            }
            (NodeType::Worker, net::EventOut::TasksExecute(mut tasks)) => {
                for task in tasks.drain(..) {
                    // TODO: expect
                    let task_id =
                        TaskId::from_bytes(task.task_id).expect("not a correct multihash");
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
            }
            // Muting those from the logs
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
        let chain = self.chain();
        let local_address = chain.local_address()?;
        match event {
            SharedStateEvent::Pending => {
                let (shared_state, workers) = join!(self.shared_state.read(), self.workers.read());

                if workers.len() > 0 {
                    let task = shared_state.tasks.get(&task_id).expect("Unknown task.");
                    let nb_unassigned = task.get_nb_unassigned().expect("Task not incomplete.");

                    let selected_offers: Vec<_> = workers
                        .iter()
                        .filter_map(
                            |(peer_id, opt)| if opt.is_some() { None } else { Some(peer_id) },
                        )
                        .take(nb_unassigned)
                        .map(|peer_id| {
                            let mut offer = man::Offer::default();
                            offer.task_id = task_id.clone().into_bytes();
                            offer.worker = peer_id.clone().into_bytes();
                            // TODO: our address
                            offer.workers_manager = peer_id.clone().into_bytes();
                            offer.payment_address = Vec::from(local_address.as_bytes());
                            offer.worker_price = 1;
                            offer.network_price = 1;
                            offer
                        })
                        .collect();

                    // TODO: check workers specs, for now we expect them all to have the same...
                    let msg = man::Proposal {
                        task_id: task_id.into_bytes(),
                        payment_address: Vec::from(local_address.as_bytes()),
                        proposal: Some(man::proposal::Proposal::Scheduling(
                            man::ProposeScheduling {
                                all_offers_senders: Vec::new(),
                                all_offers: Vec::new(),
                                selected_offers,
                            },
                        )),
                    };
                    self.spawn_event(Event::SharedStateProposal(msg)).await;
                }
            }
            SharedStateEvent::Assigned { worker } => {
                let (job_id, _) = chain.jobs_get_task(&task_id, true).await?;
                let other_data = chain.jobs_get_other_data(&job_id, true).await?;
                let (timeout, _, _) = chain.jobs_get_parameters(&job_id, true).await?;
                let (_, max_network_usage, _) =
                    chain.jobs_get_worker_parameters(&job_id, true).await?;
                let argument = chain.jobs_get_argument(&task_id, true).await?;
                self.send_msg_to_behaviour(net::EventIn::TasksExecute(
                    worker,
                    vec![TaskExecute {
                        task_id: task_id.into_bytes(),
                        program_addresses: other_data.program_addresses,
                        program_hash: other_data.program_hash,
                        program_kind: other_data.program_kind,
                        argument,
                        timeout,
                        max_network_usage,
                    }],
                ))
                .await;
            }
            SharedStateEvent::Unassigned {
                worker: _,
                workers_manager: _,
            } => (),
        }
        Ok(())
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
}
