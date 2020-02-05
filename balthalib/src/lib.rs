pub extern crate balthamisc as misc;
pub extern crate balthastore as store;
pub extern crate balthernet as net;
pub extern crate balthurner as runner;
extern crate tokio;

use futures::{executor::block_on, future, stream::StreamExt};
use std::{fmt, io, path::Path};
use tokio::fs;

use misc::NodeType;
use net::{
    identity::{error::DecodingError, Keypair},
    Multiaddr,
};

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

pub fn run(node_type: NodeType<()>, listen_addr: Multiaddr, addresses_to_dial: &[Multiaddr]) {
    let fut = async move {
        let keypair = balthernet::identity::Keypair::generate_secp256k1();
        let (swarm, mut inbound_tx) =
            net::get_swarm(node_type, keypair, listen_addr, addresses_to_dial);

        inbound_tx.send(net::EventIn::Ping).await.unwrap();

        swarm
            .for_each(|e| {
                eprintln!("S --- event: {:?}", e);
                future::ready(())
            })
            .await;
    };

    block_on(fut);
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
