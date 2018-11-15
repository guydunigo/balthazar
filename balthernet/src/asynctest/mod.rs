use tokio::prelude::*;
use tokio::runtime::Runtime;

use std::fs::File;
use std::net::SocketAddr;

use super::*;

mod message_codec;
use self::message_codec::MessageCodec;
mod shoal;
pub use self::shoal::Pid;
use self::shoal::*;

pub fn swim(local_addr: SocketAddr) -> Result<(), Error> {
    let reader = File::open("./peers.ron")?;
    let addrs: Vec<String> = ron::de::from_reader(reader).unwrap();

    // TODO: actual pid
    let local_pid: Pid = local_addr.port() as Pid;
    println!("Using pid : {}", local_pid);

    let mut runtime = Runtime::new()?;

    let (shoal, rx) = ShoalReadArc::new(local_pid, local_addr);

    shoal.swim(&mut runtime, &addrs[..])?;

    let rx_future = rx
        .for_each(|(pid, msg)| {
            println!("Shoal : {} : Received msg `{:?}`", pid, msg);
            Ok(())
        })
        .map(|_| ())
        .map_err(|_| ());

    runtime.spawn(rx_future);

    runtime
        .shutdown_on_idle()
        .wait()
        .map_err(|_| Error::TokioRuntimeError)
}
