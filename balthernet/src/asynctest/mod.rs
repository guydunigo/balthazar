use rand::random;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::collections::HashMap;
use std::fs::File;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use super::*;

mod message_codec;
use self::message_codec::MessageCodec;
mod peer;
use self::peer::*;
mod client;
mod listener;

// TODO: personnal PID (possibly public key)
// TODO: subfunctions
pub fn swim(local_addr: SocketAddr) -> Result<(), Error> {
    let reader = File::open("./peers.ron")?;
    let addrs: Vec<String> = ron::de::from_reader(reader).unwrap();

    // TODO: actual pid
    // create pid:
    let local_pid: Pid = random();
    println!("Using pid : {}", local_pid);

    let mut runtime = Runtime::new()?;

    let peers: PeersMapArcMut = Arc::new(Mutex::new(HashMap::new()));

    addrs
        .iter()
        .map(parse_socket_addr)
        .filter_map(|addr| match addr {
            Ok(addr) => Some(addr),
            Err(err) => {
                eprintln!("{:?}", err);
                None
            }
        })
        .filter(|addr| *addr != local_addr)
        .for_each(|addr| {
            let client_future = client::try_connecting_at_interval(local_pid, addr, peers.clone());
            runtime.spawn(client_future);
        });

    let listener = listener::bind(&local_addr)?;
    let listener_future = listener::listen(local_pid, peers, listener);
    runtime.spawn(listener_future);

    runtime
        .shutdown_on_idle()
        .wait()
        .map_err(|_| Error::TokioRuntimeError)
}
