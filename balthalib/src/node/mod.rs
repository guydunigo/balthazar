//! Grouping module for all balthazar sub-modules.
// TODO: remove allws
// #![allow(unused_imports)]
// #![allow(dead_code)]

extern crate async_ctrlc;

use futures::{
    channel::{
        mpsc::{channel, Receiver, Sender},
        oneshot,
    },
    future, join, poll, select, FutureExt, SinkExt, Stream, StreamExt,
};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    convert::TryFrom,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{runtime::Runtime, sync::RwLock};

use async_ctrlc::CtrlC;
use chain::{Chain, JobsEvent};
use misc::{
    job::{try_bytes_to_address, DefaultHash, JobId, ProgramKind, TaskId},
    multihash::{Keccak256, Multihash},
    shared_state::{SharedState, Task, TaskCompleteness},
    WorkerSpecs,
};
use net::PeerRc;
use proto::{
    manager as man,
    worker::{TaskErrorKind, TaskExecute},
    NodeType, TaskStatus,
};
use run::{Executor, WasmExecutor};
use store::{FetchStorage, StoragesWrapper};

use super::{BalthazarConfig, Error};
mod shared_state;

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

#[derive(Debug)]
enum Event {
    ChainJobs(chain::JobsEvent),
    Swarm(net::EventOut),
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
    Error,
}

impl fmt::Display for LogKind {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let letter = match self {
            LogKind::Swarm => "S",
            LogKind::Worker => "W",
            LogKind::Manager => "M",
            LogKind::Blockchain => "B",
            LogKind::SharedState => "T",
            LogKind::Error => "E",
        };
        write!(fmt, "{}", letter)
    }
}

#[derive(Clone)]
struct Balthazar {
    config: Arc<BalthazarConfig>,
    event_tx: Sender<Event>,
    shared_state_tx: Sender<man::Proposal>,
    swarm_in: Sender<net::EventIn>,
    shared_state: Arc<RwLock<SharedState>>,
    // pending_tasks: Arc<RwLock<VecDeque<(TaskId, Option<PeerRc>)>>>,
}
/*
{
    keypair: balthernet::identity::Keypair,
    swarm_in: Sender<net::EventIn>,
    store: StoragesWrapper,
}
*/

impl Balthazar {
    fn new(
        config: BalthazarConfig,
        event_tx: Sender<Event>,
        shared_state_tx: Sender<man::Proposal>,
        swarm_in: Sender<net::EventIn>,
    ) -> Self {
        Balthazar {
            config: Arc::new(config),
            event_tx,
            shared_state_tx,
            swarm_in,
            shared_state: Arc::new(RwLock::new(SharedState::default())),
            // pending_tasks: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    /*
    async fn pending_tasks<'a>(
        &'a self,
    ) -> impl 'a + std::ops::Deref<Target = VecDeque<(TaskId, Option<PeerRc>)>> {
        self.pending_tasks.read().await
    }

    async fn pending_tasks_mut<'a>(
        &'a self,
    ) -> impl 'a + std::ops::DerefMut<Target = VecDeque<(TaskId, Option<PeerRc>)>> {
        self.pending_tasks.write().await
    }
    */

    // TODO: re-create it each time ?
    fn chain(&self) -> Chain {
        Chain::new(self.config.chain())
    }

    /*
    /// Get the next task that doesn't have any worker.
    async fn next_pending_task(&self) -> Option<TaskId> {
        let mut pending_tasks = self.pending_tasks_mut().await;
        let next_index =
            pending_tasks
                .iter()
                .enumerate()
                .find_map(|(i, (_, o))| if o.is_none() { Some(i) } else { None });
        if let Some(i) = next_index {
            let (t, _) = pending_tasks
                .remove(i)
                .expect("We just found the value, it should exists.");
            Some(t)
        } else {
            None
        }
    }
    */

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
        println!("Starting as {:?}...", config.node_type());

        let (event_tx, event_rx) = channel(CHANNEL_SIZE);
        let (shared_state_tx, shared_state_rx) = channel(CHANNEL_SIZE);

        let specs = WorkerSpecs::default();
        let keypair = balthernet::identity::Keypair::generate_secp256k1();
        let (swarm_in, swarm_out) = net::get_swarm(keypair.clone(), config.net(), Some(&specs));

        let balth = Balthazar::new(config, event_tx, shared_state_tx, swarm_in);

        /*
        let chain = balth.chain();
        let chain_fut = chain.jobs_subscribe().await?.for_each(|e| {
            async {
                let event = match e {
                    Ok(evt) => Event::ChainJobs(evt),
                    Err(e) => Event::Error(e.into()),
                };
                spawn_event(balth.tx.clone(), event).await;
            }
            .boxed()
        });
        */

        // TODO: concurrent ?
        let channel_fut = event_rx.for_each(|e| balth.clone().handle_event(e));
        let ctrlc_fut = balth.clone().handle_ctrlc();
        // spawn_event(balth.tx.clone(), Event::Swarm(e))
        let swarm_fut = swarm_out.for_each(|e| balth.clone().handle_swarm_event(e));
        let shared_state_fut = balth.clone().handle_shared_state(shared_state_rx);

        select! {
            _ = swarm_fut.fuse() => (),
            _ = channel_fut.fuse() => (),
            _ = shared_state_fut.fuse() => (),
            _ = ctrlc_fut.fuse() => (),
        }

        eprintln!("Bye...");
        Ok(())
    }

    // TODO: ref as mutable ? or just clone tx ?
    async fn spawn_event(&self, event: Event) {
        let mut tx = self.event_tx.clone();
        if let Err(e) = tx.send(event).await {
            panic!("{:?}", Error::EventChannelError(e));
        }
    }

    async fn spawn_log(&self, kind: LogKind, msg: String) {
        self.spawn_event(Event::Log { kind, msg }).await
    }

    /// Handle all inner events.
    async fn handle_event(self, event: Event) {
        match event {
            // Event::Swarm(e) => self.handle_swarm_event(e).await,
            // Event::ChainJobs(e) => self.handle_chain_event(e).await,
            Event::Log { .. } => eprintln!("{}", event),
            _ => unimplemented!(),
        }
    }

    /*
    /// Handle events coming out of Swarm.
    async fn handle_chain_event(self, event: chain::JobsEvent) {
        self.spawn_log(LogKind::Blockchain, format!("{}", event))
            .await;
        match event {
            // let mut free_workers_iter = self.find_available_worker().await;
            chain::JobsEvent::TaskPending { task_id } => {
                let mut p = self.pending_tasks_mut().await;
                p.push_back((task_id, None))
            }
            _ => (),
        }
    }
    */

    // Accepting everything...
    // TODO: Channel out to send notif to wake up other parts ?
    // TODO: check proof messages...
    async fn handle_shared_state(self, shared_state_rx: Receiver<man::Proposal>) {
        shared_state_rx
            .for_each(|p| self.check_and_apply_proposal(p))
            .await
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
                self.spawn_log(
                    LogKind::Swarm,
                    format!("event: {:?}", net::EventOut::WorkerNew(peer_id.clone())),
                )
                .await;
                if let Some((wasm, args)) = self.config.wasm() {
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
                            String::from_utf8_lossy(wasm),
                            args_str,
                            peer_id
                        ),
                    )
                    .await;
                    self.send_msg_to_behaviour(net::EventIn::TasksExecute(peer_id, tasks))
                        .await
                }
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
                    let string_program_address =
                        String::from_utf8_lossy(&task.program_addresses[0][..]);
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
}
