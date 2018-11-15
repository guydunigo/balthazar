use futures::sync::mpsc;
use tokio::io;
use tokio::runtime::Runtime;

use balthmessage::Message;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock, Weak};

use super::{Error, MessageCodec};
mod client;
mod listener;
mod peer;
pub use self::peer::*;
use super::super::parse_socket_addr;

// TODO: think abuot this size
const SHOAL_MPSC_SIZE: usize = 32;

// TODO: Good for big messages ?
pub type MsgsMapArcMut = Arc<Mutex<HashMap<Message, Vec<Pid>>>>;

// ------------------------------------------------------------------
/// # Shoal
///
/// This struct stores the data needed across the network (local infos, peers, ...).
/// It provides also methods to interract with all the peers directly (broadcast, ...).
///
/// This structure is not intended to be used as mutable, because there is no need for it.
///
/// To use it in an multithreaded context, see : [`ShoalReadArc`]
///
/// > **Shoal** because this *materializes* the local *cephalopode* and its peers *swimming* together and
/// communicating.
pub struct Shoal {
    local_pid: Pid,
    local_addr: SocketAddr,
    tx: MpscSenderMessage,
    // TODO: peers accessor that automatically clones it ?
    peers: PeersMapArcMut,
    // TODO: Clean sometimes ?
    // TODO: have a way to monitor size ?
    msgs_received: MsgsMapArcMut,
}

impl Shoal {
    pub fn new(local_pid: Pid, local_addr: SocketAddr) -> (Self, MpscReceiverMessage) {
        let (tx, rx) = mpsc::channel(SHOAL_MPSC_SIZE);
        (
            Shoal {
                local_pid,
                local_addr,
                tx,
                peers: Arc::new(Mutex::new(HashMap::new())),
                msgs_received: Arc::new(Mutex::new(HashMap::new())),
            },
            rx,
        )
    }

    pub fn local_pid(&self) -> Pid {
        self.local_pid
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn peers(&self) -> PeersMapArcMut {
        self.peers.clone()
    }

    pub fn msgs_received(&self) -> MsgsMapArcMut {
        self.msgs_received.clone()
    }

    pub fn tx(&self) -> MpscSenderMessage {
        self.tx.clone()
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
}

// ------------------------------------------------------------------
/// # ShoalReadArc
///
/// Simple wrapper around an Arc<RwLock<Shoal>>, so it is easier to use (no `.read().unwrap()`).

#[derive(Clone)]
pub struct ShoalReadArc {
    inner: Arc<RwLock<Shoal>>,
}

impl From<Shoal> for ShoalReadArc {
    fn from(s: Shoal) -> Self {
        ShoalReadArc {
            inner: Arc::new(RwLock::new(s)),
        }
    }
}

impl ShoalReadArc {
    pub fn new(local_pid: Pid, local_addr: SocketAddr) -> (Self, MpscReceiverMessage) {
        let (shoal, rx) = Shoal::new(local_pid, local_addr);
        (
            ShoalReadArc {
                inner: Arc::new(RwLock::new(shoal)),
            },
            rx,
        )
    }

    pub fn downgrade(&self) -> ShoalReadWeak {
        ShoalReadWeak {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub fn lock(&self) -> impl Deref<Target = Shoal> + '_ {
        self.inner
            .read()
            .expect("Could not lock the Shaol object for reading.")
    }

    pub fn start_clients(&self, runtime: &mut Runtime, addrs: &[String]) {
        let shoal = self.lock();
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
            .filter(|peer_addr| *peer_addr != shoal.local_addr)
            .for_each(|peer_addr| {
                let client_future = client::try_connecting_at_interval(self.clone(), peer_addr);
                runtime.spawn(client_future);
            });
    }

    // TODO: Stop program in case of binding issue or can still work ?
    pub fn start_listener(&self, runtime: &mut Runtime) -> Result<(), io::Error> {
        let shoal = self.lock();
        let listener = listener::bind(&shoal.local_addr)?;
        let listener_future = listener::listen(self.clone(), listener);

        runtime.spawn(listener_future);
        Ok(())
    }

    pub fn swim(&self, runtime: &mut Runtime, addrs: &[String]) -> Result<(), Error> {
        self.start_clients(runtime, addrs);
        self.start_listener(runtime)?;

        Ok(())
    }
}

// ------------------------------------------------------------------
/// ShoalReadWeak
#[derive(Clone)]
pub struct ShoalReadWeak {
    inner: Weak<RwLock<Shoal>>,
}

impl ShoalReadWeak {
    fn upgrade(&self) -> ShoalReadArc {
        ShoalReadArc {
            inner: self.inner.upgrade().expect(
                "Could not upgrade reference, it has been dropped (which should not happen).",
            ),
        }
    }
}
