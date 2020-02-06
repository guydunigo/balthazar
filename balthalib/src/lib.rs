pub extern crate balthamisc as misc;
pub extern crate balthastore as store;
pub extern crate balthernet as net;
pub extern crate balthurner as run;
extern crate tokio;

use futures::{executor::block_on, future, FutureExt, StreamExt};
use std::{fmt, future::Future, io, path::Path};
use tokio::{fs, sync::mpsc::Sender};

use misc::NodeType;
use net::{
    identity::{error::DecodingError, Keypair},
    Multiaddr,
};
use run::Runner;
use store::Storage;

#[derive(Debug)]
pub enum BalthazarError {
    ReadKeyFileError(io::Error),
    KeyDecodingError(DecodingError),
}

impl fmt::Display for BalthazarError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self)
    }
}

// TODO: cleaner and in self module
pub async fn get_keypair(keyfile_path: &Path) -> Result<Keypair, BalthazarError> {
    let mut bytes = fs::read(keyfile_path)
        .await
        .map_err(BalthazarError::ReadKeyFileError)?;
    Keypair::rsa_from_pkcs8(&mut bytes).map_err(BalthazarError::KeyDecodingError)
}

pub fn run(node_type: NodeType, listen_addr: Multiaddr, addresses_to_dial: &[Multiaddr]) {
    let fut = async move {
        let keypair = balthernet::identity::Keypair::generate_secp256k1();
        let (swarm, mut inbound_tx) =
            net::get_swarm(node_type, keypair, listen_addr, addresses_to_dial);

        inbound_tx.send(net::EventIn::Ping).await.unwrap();

        swarm
            .for_each(|e| handle_event(e, inbound_tx.clone()))
            .await;
    };

    block_on(fut);
}

/// Handle events coming out of Swarm:
fn handle_event(
    event: net::EventOut,
    mut inbound_tx: Sender<net::EventIn>,
) -> impl Future<Output = ()> {
    eprintln!("S --- event: {:?}", event);

    match event {
        net::EventOut::Handler(
            peer_id,
            net::HandlerOut::ExecuteTask {
                job_addr,
                argument,
                request_id,
            },
        ) => async move {
            let storage = store::StoragesWrapper::default();
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
                            inbound_tx
                                .send(net::EventIn::Handler(
                                    peer_id,
                                    net::HandlerIn::TaskResult { result, request_id },
                                ))
                                .await
                                .expect("BalthBehaviour inbound_tx has a problem (dropped?)");
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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
