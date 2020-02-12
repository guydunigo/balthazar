pub extern crate balthamisc as misc;
pub extern crate balthastore as store;
pub extern crate balthernet as net;
pub extern crate balthurner as run;
extern crate tokio;

use futures::{future, join, FutureExt, StreamExt};
use std::{fmt, future::Future, io, path::Path};
use tokio::{fs, runtime::Runtime, sync::mpsc::Sender};

use misc::NodeType;
use net::identity::{error::DecodingError, Keypair};
use run::Runner;
use store::{Storage, StoragesWrapper};

mod config;
pub use config::BalthazarConfig;

const TEST_JOB_ADDR: &[u8] = b"/ipfs/QmSpTZ2RmiwDgxRuG7KsFFRH6xxkSF2vtGYHu7PJoGoD3g";
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
        let keypair = balthernet::identity::Keypair::generate_secp256k1();
        let (swarm_in, swarm_out) = net::get_swarm(keypair.clone(), config.net());
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
            swarm_out.for_each(|e| Balthazar::handle_event(*config.node_type(), swarm_in.clone(), e));

        join!(swarm_fut);

        Ok(())
    }

    /// Handle events coming out of Swarm:
    fn handle_event(
        node_type: NodeType,
        swarm_in: Sender<net::EventIn>,
        event: net::EventOut,
    ) -> impl Future<Output = ()> {
        eprintln!("S --- event: {:?}", event);

        match (node_type, event) {
            (NodeType::Manager, net::EventOut::PeerHasNewType(peer_id, NodeType::Worker)) => {
                eprintln!(
                    "M --- Sending task `{}` with parameters `{}` to peer `{}`",
                    String::from_utf8_lossy(TEST_JOB_ADDR),
                    6,
                    peer_id
                );
                send_msg_to_behaviour(
                    swarm_in,
                    net::EventIn::ExecuteTask {
                        peer_id,
                        job_addr: Vec::from(TEST_JOB_ADDR),
                        argument: 6,
                    },
                )
                .boxed()
            }
            (_, net::EventOut::Handler(peer_id, net::HandlerOut::TaskResult { result, .. })) => {
                eprintln!("S --- Result from peer `{}`: `{}`", peer_id, result);
                future::ready(()).boxed()
            }
            (
                NodeType::Manager,
                net::EventOut::Handler(peer_id, net::HandlerOut::ExecuteTask { .. }),
            ) => {
                eprintln!(
                    "M --- Manager received ExecuteTask from peer `{}`, ignoring...",
                    peer_id
                );
                future::ready(()).boxed()
            }
            (
                NodeType::Worker,
                net::EventOut::Handler(
                    peer_id,
                    net::HandlerOut::ExecuteTask {
                        job_addr,
                        argument,
                        request_id,
                    },
                ),
            ) => async move {
                let storage = StoragesWrapper::default();
                let string_job_addr = String::from_utf8_lossy(&job_addr[..]);
                eprintln!("W --- will get program `{}`...", string_job_addr);
                match storage.get(&job_addr[..]).await {
                    Ok(wasm) => {
                        eprintln!("W --- received program `{}`.", string_job_addr);
                        eprintln!(
                            "W --- spawning wasm executor for `{}` with argument `{}`...",
                            string_job_addr, argument
                        );
                        match Runner::new(wasm).run_async(argument).await {
                            Ok(result) => {
                                eprintln!(
                                    "W --- result for `{}` with `{}`: `{}`",
                                    string_job_addr, argument, result
                                );
                                send_msg_to_behaviour(
                                    swarm_in,
                                    net::EventIn::Handler(
                                        peer_id,
                                        net::HandlerIn::TaskResult { result, request_id },
                                    ),
                                )
                                .await;
                            }
                            Err(error) => {
                                eprintln!(
                                    "W --- error for `{}` with `{}`: `{:?}`",
                                    String::from_utf8_lossy(&job_addr[..]),
                                    argument,
                                    error
                                );
                            }
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "W --- error while fetching `{}`: `{:?}`",
                            String::from_utf8_lossy(&job_addr[..]),
                            error
                        );
                    }
                }
            }
            .boxed(),
            _ => future::ready(()).boxed(),
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
        .expect("BalthBehaviour inbound channel has a problem (dropped?)")
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
