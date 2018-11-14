use tokio::net::TcpStream;
use tokio::prelude::*;

use balthmessage::Message;

use std::net::SocketAddr;
use std::sync::{Arc, Weak, Mutex, RwLock, RwLockReadGuard};
use std::collections::HashMap;

use super::Error;
use super::peer::*;

pub type ShoalArcRwLock = Arc<RwLock<Shoal>>;
pub type ShoalWeakRwLock = Weak<RwLock<Shoal>>;
pub type ShoalRwLockRead<'a> = RwLockReadGuard<'a, Shoal>;
// TODO: Good for big messages ?
pub type LastMsgsMapArcMut = Arc<Mutex<HashMap<Message, Vec<Pid>>>>;

// TODO: Peer uses those values directly instead of cloning them ?
// TODO: wrapper around the rwlock + clone as Arc ?
/// This struct stores the data needed across the network (local infos, peers, ...).
/// It provides also methods to interract with all the peers directly (broadcast, ...).
///
/// > **Shoal** because this *materializes* the local *cephalopode* and its peers *swimming* together and
/// communicating.
#[derive(Debug)]
pub struct Shoal {
    // TODO: no pub? to prevent any modification
    pub local_pid: Pid,
    pub local_addr: SocketAddr,
    // TODO: peers accessor that automatically clones it ?
    pub peers: PeersMapArcMut,
    // TODO: Clean sometimes ?
    // TODO: have a way to monitor size ?
    pub msgs_received: LastMsgsMapArcMut,
}

impl Shoal {
    pub fn new(local_pid: Pid, local_addr: SocketAddr) -> Self {
        Shoal {
            local_pid,
            local_addr,
            peers: Arc::new(Mutex::new(HashMap::new())),
            msgs_received: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Should always be `read` and not `write`
    /// TODO: enforce that ?
    pub fn new_arc_rwlock(local_pid: Pid, local_addr: SocketAddr) -> ShoalArcRwLock {
        Arc::new(RwLock::new(Shoal::new(local_pid, local_addr)))
    }

    pub fn broadcast(&self, msg: Message, exclude: &[Pid]) {
        let peers = self.peers.lock().unwrap();

        peers.iter().for_each(|(pid, peer)| {
            let mut peer = peer.lock().unwrap();
            if exclude.iter().find(|expid| *expid == pid).is_none() {
                peer.send_and_spawn(msg.clone());
            }
        });
    }

    // TODO: listen + client as methods
}
