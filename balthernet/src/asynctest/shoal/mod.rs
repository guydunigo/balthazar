use futures::future::Shared;
use futures::sync::{mpsc, oneshot};
use tokio::io;
use tokio::prelude::*;
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
pub type OrphanMsgsMapArcMut = Arc<
    Mutex<
        HashMap<
            Pid,
            (
                oneshot::Sender<PeerArcMut>,
                Shared<oneshot::Receiver<PeerArcMut>>,
            ),
        >,
    >,
>;

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
    orphan_messages: OrphanMsgsMapArcMut,
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
                orphan_messages: Arc::new(Mutex::new(HashMap::new())),
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

    pub fn send_to(&self, peer_pid: Pid, msg: Message) -> Result<(), Error> {
        let peers = self.peers.lock().unwrap();

        match peers.get(&peer_pid) {
            Some(peer) => {
                let mut peer = peer.lock().unwrap();
                peer.send_and_spawn(msg.clone());
                Ok(())
            }
            None => {
                // Err(Error::PeerNotFound(peer_pid)),
                let mut om = self.orphan_messages.lock().unwrap();
                let ready_rx = match om.get(&peer_pid) {
                    Some((_, ready_rx)) => ready_rx.clone(),
                    None => {
                        let (ready_tx, ready_rx) = oneshot::channel();
                        let ready_rx = ready_rx.shared();

                        om.insert(peer_pid, (ready_tx, ready_rx.clone()));
                        ready_rx
                    }
                };

                let future = ready_rx
                    .map_err(|err| Error::OneShotError(err))
                    .and_then(|peer| peer.lock().unwrap().send(msg))
                    .map(|_| ())
                    // TODO: discarding error, really ?
                    .map_err(|_| ());
                tokio::spawn(future);

                Ok(())
            }
        }
    }

    pub fn send_to_with_runtime(
        &self,
        runtime: &mut Runtime,
        peer_pid: Pid,
        msg: Message,
    ) -> Result<(), Error> {
        let peers = self.peers.lock().unwrap();

        match peers.get(&peer_pid) {
            Some(peer) => {
                let mut peer = peer.lock().unwrap();
                let send_future = peer.send(msg).map(|_| ()).map_err(|_| ());
                runtime.spawn(send_future);

                Ok(())
            }
            None => Err(Error::PeerNotFound(peer_pid)),
        }
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

    pub fn insert_peer(&self, peer: PeerArcMut) {
        let mut peers = self.peers.lock().unwrap();
        let peer_pid = peer.lock().unwrap().pid();
        let mut om = self.orphan_messages.lock().unwrap();

        peers.insert(peer_pid, peer.clone());

        if let Some((ready_tx, _)) = om.remove(&peer_pid) {
            ready_tx
                .send(peer)
                .expect("Peer : Could not send peer to orphan messages.");
        }
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
