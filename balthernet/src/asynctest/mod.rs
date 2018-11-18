use tokio::prelude::*;
use tokio::runtime::Runtime;

use super::*;

mod message_codec;
use self::message_codec::MessageCodec;
pub mod shoal;
pub use self::shoal::Pid;
use self::shoal::*;

/// Test function to start only the p2p network
pub fn swim(runtime: &mut Runtime, _shoal: ShoalReadArc, shoal_rx: MpscReceiverMessage) {
    let rx_future = shoal_rx
        .for_each(|(pid, msg)| {
            if let Message::Job(_, _) = msg {
            } else {
                println!("Shoal : {} : Received msg `{:?}`", pid, msg);
            }
            Ok(())
        })
        .map(|_| ())
        .map_err(|_| ());

    runtime.spawn(rx_future);
}
