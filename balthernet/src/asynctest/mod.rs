use tokio::prelude::*;
use tokio::runtime::Runtime;

use super::*;

pub mod shoal;
pub use self::shoal::PeerId;
use self::shoal::*;

/// Test function to start only the p2p network
pub fn swim(runtime: &mut Runtime, _shoal: ShoalReadArc, shoal_rx: MpscReceiverMessage) {
    let rx_future = shoal_rx
        .for_each(|(_pid, _msg)| {
            // println!("Shoal : {} : Received msg `{:?}`", pid, msg);
            Ok(())
        })
        .map(|_| ())
        .map_err(|_| ());

    runtime.spawn(rx_future);
}
