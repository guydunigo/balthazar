pub extern crate balthamisc as misc;
pub extern crate balthastore as store;
pub extern crate balthernet as net;
pub extern crate balthurner as run;
extern crate tokio;

use futures::{future, join, FutureExt, StreamExt};
use std::{
    collections::HashMap,
    fmt,
    future::Future,
    io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{fs, runtime::Runtime, sync::mpsc::Sender};

use misc::{NodeType, TaskErrorKind, TaskExecute, TaskStatus, WorkerSpecs};
use net::identity::{error::DecodingError, Keypair};
use run::{Runner, WasmRunner};
use store::{Storage, StoragesWrapper};

mod config;
pub use config::BalthazarConfig;

// const CHANNEL_SIZE: usize = 1024;

#[derive(Debug)]
pub enum Error {
    KeyPairReadFileError(io::Error),
    KeyPairDecodingError(DecodingError),
    StorageCreationError(store::StoragesWrapperCreationError),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self)
    }
}

impl From<store::StoragesWrapperCreationError> for Error {
    fn from(src: store::StoragesWrapperCreationError) -> Self {
        Error::StorageCreationError(src)
    }
}

// TODO: cleaner and in self module
pub async fn get_keypair(keyfile_path: &Path) -> Result<Keypair, Error> {
    let mut bytes = fs::read(keyfile_path)
        .await
        .map_err(Error::KeyPairReadFileError)?;
    Keypair::rsa_from_pkcs8(&mut bytes).map_err(Error::KeyPairDecodingError)
}

/*
#[derive(Debug)]
enum BalthEvent {
    SwarmEvent(net::EventOut),
    SendEventToSwarm(net::EventIn),
    ManagerEvent,
}
*/

pub struct Balthazar {
    /*
keypair: balthernet::identity::Keypair,
swarm_in: Sender<net::EventIn>,
config: BalthazarConfig,
events_in: Sender<BalthEvent>,
events: Receiver<BalthEvent>,
store: StoragesWrapper,
*/}

pub fn run(config: BalthazarConfig) -> Result<(), Error> {
    Runtime::new().unwrap().block_on(Balthazar::run(config))
}

impl Balthazar {
    pub async fn run(config: BalthazarConfig) -> Result<(), Error> {
        println!("Starting as {:?}...", config.node_type());

        let specs = WorkerSpecs::default();
        let keypair = balthernet::identity::Keypair::generate_secp256k1();
        let (swarm_in, swarm_out) = net::get_swarm(keypair.clone(), config.net(), Some(&specs));
        /*
        let (events_in, events) = channel(CHANNEL_SIZE);
        let store = StoragesWrapper::new_with_config(config.storage())?;

        let balth = Balthazar {
            keypair,
            swarm_in,
            config,
            events_in,
            events,
            store,
        };
        */

        let swarm_fut =
            // swarm_out.for_each(|e| push_event(balth.events_in.clone(), BalthEvent::SwarmEvent(e)));
            swarm_out.for_each_concurrent(None, |e| Balthazar::handle_event(&config, swarm_in.clone(), e));

        join!(swarm_fut);

        Ok(())
    }

    /// Handle events coming out of Swarm:
    fn handle_event(
        config: &BalthazarConfig,
        swarm_in: Sender<net::EventIn>,
        event: net::EventOut,
    ) -> impl Future<Output = ()> {
        match (config.node_type(), event) {
            (NodeType::Manager, net::EventOut::WorkerNew(peer_id)) => {
                if let Some((wasm, args)) = config.wasm() {
                    let mut tasks = HashMap::new();
                    tasks.insert(
                        wasm.clone(),
                        TaskExecute {
                            job_id: wasm.clone(),
                            task_id: wasm.clone(),
                            job_addr: vec![wasm.clone()],
                            arguments: args.clone(),
                            timeout: 100,
                        },
                    );
                    eprintln!(
                        "M --- Sending task `{}` with parameters `{}` to worker `{}`",
                        String::from_utf8_lossy(wasm),
                        String::from_utf8_lossy(args),
                        peer_id
                    );
                    send_msg_to_behaviour(swarm_in, net::EventIn::TasksExecute(peer_id, tasks))
                        .boxed()
                } else {
                    future::ready(()).boxed()
                }
            }
            (
                _,
                net::EventOut::TaskStatus {
                    peer_id,
                    task_id,
                    status,
                },
            ) => {
                eprintln!(
                    "M --- Task status from peer `{}` for task `{}`: `{:?}`",
                    peer_id,
                    String::from_utf8_lossy(&task_id[..]),
                    status
                );
                future::ready(()).boxed()
            }
            (NodeType::Worker, net::EventOut::TasksExecute(tasks)) => async move {
                for task in tasks.values() {
                    send_msg_to_behaviour(
                        swarm_in.clone(),
                        net::EventIn::TaskStatus(task.task_id.clone(), TaskStatus::Pending),
                    )
                    .await;
                    let storage = StoragesWrapper::default();
                    let string_job_addr = String::from_utf8_lossy(&task.job_addr[0][..]);
                    let string_arguments = String::from_utf8_lossy(&task.arguments[..]);

                    eprintln!("W --- will get program `{}`...", string_job_addr);
                    match storage.get(&task.job_addr[0][..]).await {
                        Ok(wasm) => {
                            eprintln!("W --- received program `{}`.", string_job_addr);
                            eprintln!(
                                "W --- spawning wasm executor for `{}` with argument `{}`...",
                                string_job_addr, string_arguments,
                            );

                            send_msg_to_behaviour(
                                swarm_in.clone(),
                                net::EventIn::TaskStatus(
                                    task.task_id.clone(),
                                    TaskStatus::Started(
                                        SystemTime::now()
                                            .duration_since(UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs()
                                            as i64,
                                    ),
                                ),
                            )
                            .await;

                            match WasmRunner::run_async(&wasm[..], &task.arguments[..]).await {
                                Ok(result) => {
                                    eprintln!(
                                        "W --- task result for `{}` with `{}`: `{}`",
                                        string_job_addr,
                                        string_arguments,
                                        String::from_utf8_lossy(&result[..])
                                    );
                                    send_msg_to_behaviour(
                                        swarm_in.clone(),
                                        net::EventIn::TaskStatus(
                                            task.task_id.clone(),
                                            TaskStatus::Completed(result),
                                        ),
                                    )
                                    .await;
                                }
                                Err(error) => {
                                    send_msg_to_behaviour(
                                        swarm_in.clone(),
                                        net::EventIn::TaskStatus(
                                            task.task_id.clone(),
                                            TaskStatus::Error(TaskErrorKind::Running),
                                        ),
                                    )
                                    .await;
                                    eprintln!(
                                        "W --- task error for `{}` with `{}`: `{:?}`",
                                        string_job_addr, string_arguments, error
                                    );
                                }
                            }
                        }
                        Err(error) => {
                            send_msg_to_behaviour(
                                swarm_in.clone(),
                                net::EventIn::TaskStatus(
                                    task.task_id.clone(),
                                    TaskStatus::Error(TaskErrorKind::Download),
                                ),
                            )
                            .await;
                            eprintln!(
                                "W --- error while fetching `{}`: `{:?}`",
                                string_job_addr, error
                            );
                        }
                    }
                }
            }
            .boxed(),
            (_, event) => {
                eprintln!("S --- event: {:?}", event);
                future::ready(()).boxed()
            }
        }
    }
}

/*
async fn push_event(mut events_in: Sender<BalthEvent>, evt: BalthEvent) {
    events_in
        .send(evt)
        .await
        .expect("Event channel closed in Balthazar!")
}
*/

async fn send_msg_to_behaviour(mut swarm_in: Sender<net::EventIn>, msg: net::EventIn) {
    swarm_in
        .send(msg)
        .await
        .expect("BalthBehaviour inbound channel has a problem (dropped?)");
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
