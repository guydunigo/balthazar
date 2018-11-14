use tokio::prelude::*;
use tokio::runtime::Runtime;
use futures::sync::mpsc;

use std::collections::HashMap;
use std::fs::File;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use super::*;

mod message_codec;
use self::message_codec::MessageCodec;
mod peer;
use self::peer::*;
mod shoal;
use self::shoal::*;
mod client;
mod listener;

pub fn start_clients(runtime: &mut Runtime, shoal: ShoalArcRwLock, addrs: &[String]) {
    let local_addr = shoal.read().unwrap().local_addr;
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
        .filter(|peer_addr| *peer_addr != local_addr)
        .for_each(|peer_addr| {
            let client_future = client::try_connecting_at_interval(shoal.clone(), peer_addr);
            runtime.spawn(client_future);
        });
}

// TODO: Stop program in case of binding issue or can still work ?
pub fn start_listener(runtime: &mut Runtime, shoal: ShoalArcRwLock) -> Result<(), io::Error> {
    let listener = {
        let shoal_read = shoal.read().unwrap();
        listener::bind(&shoal_read.local_addr)?
    };
    let listener_future = listener::listen(shoal, listener);

    runtime.spawn(listener_future);
    Ok(())
}

pub fn swim(local_addr: SocketAddr) -> Result<(), Error> {
    let reader = File::open("./peers.ron")?;
    let addrs: Vec<String> = ron::de::from_reader(reader).unwrap();

    // TODO: actual pid
    let local_pid: Pid = local_addr.port() as Pid;
    println!("Using pid : {}", local_pid);

    let mut runtime = Runtime::new()?;

    let shoal = Shoal::new_arc_rwlock(local_pid, local_addr);

    start_clients(&mut runtime, shoal.clone(), &addrs[..]);

    start_listener(&mut runtime, shoal)?;

    runtime
        .shutdown_on_idle()
        .wait()
        .map_err(|_| Error::TokioRuntimeError)
}
