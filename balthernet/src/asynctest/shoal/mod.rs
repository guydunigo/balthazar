use futures::future::Shared;
use futures::sync::{mpsc, oneshot};
use tokio::codec::Framed;
use tokio::io;
use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use balthmessage::Message;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock, Weak};

use super::{Error, MessageCodec};
mod client;
mod listener;
mod peer;
pub use self::peer::*;

// TODO: think abuot this size
const SHOAL_MPSC_SIZE: usize = 32;

// TODO: Good for big messages ?
pub type MsgsMapArcMut = Arc<Mutex<HashMap<Message, Vec<PeerId>>>>;
pub type OrphanMsgsMapArcMut = Arc<
    Mutex<
        HashMap<
            PeerId,
            (
                oneshot::Sender<PeerArcMut>,
                Shared<oneshot::Receiver<PeerArcMut>>,
            ),
        >,
    >,
>;
pub type ReceivedMsgsSetArcMut = Arc<Mutex<HashSet<(PeerId, Message)>>>;

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
#[derive(Debug)]
pub struct Shoal {
    local_pid: PeerId,
    local_addr: SocketAddr,
    tx: MpscSenderMessage,
    // TODO: peers accessor that automatically clones it ?
    peers: PeersMapArcMut,
    // TODO: Clean sometimes ?
    // TODO: have a way to monitor size ?
    msgs_received: MsgsMapArcMut,
    orphan_messages: OrphanMsgsMapArcMut,
    received_messages: ReceivedMsgsSetArcMut,
}

impl Shoal {
    pub fn new(local_pid: PeerId, local_addr: SocketAddr) -> (Self, MpscReceiverMessage) {
        let (tx, rx) = mpsc::channel(SHOAL_MPSC_SIZE);
        (
            Shoal {
                local_pid,
                local_addr,
                tx,
                peers: Arc::new(Mutex::new(HashMap::new())),
                msgs_received: Arc::new(Mutex::new(HashMap::new())),
                orphan_messages: Arc::new(Mutex::new(HashMap::new())),
                received_messages: Arc::new(Mutex::new(HashSet::new())),
            },
            rx,
        )
    }

    pub fn local_pid(&self) -> PeerId {
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

    pub fn send_to(&self, peer_pid: PeerId, msg: Message) {
        // TODO: do not discard errors ...
        let future = self
            .send_to_future(peer_pid, msg)
            .map(|_| ())
            .map_err(|_| ());
        tokio::spawn(future);
    }

    pub fn send_to_future(
        &self,
        peer_pid: PeerId,
        msg: Message,
    ) -> Box<Future<Item = Framed<TcpStream, MessageCodec>, Error = Error> + Send> {
        let peers = self.peers.lock().unwrap();

        match peers.get(&peer_pid) {
            Some(peer) => {
                let mut peer = peer.lock().unwrap();
                Box::new(peer.send(msg.clone()))
            }
            None => {
                // Err(Error::PeerNotFound(peer_pid)),
                println!(
                    "Shoal : Setting msg `{:?}` to be sent when peer `{}` is created.",
                    &format!("{:?}", msg)[..4],
                    peer_pid
                );

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
                    .and_then(|peer| peer.lock().unwrap().send(msg));

                Box::new(future)
            }
        }
    }

    /// This function send `msg` to all connected peers only if it wasn't already broadcasted.
    // TODO: what happens if someone wants to broadcast the same msg twice (`Idle` for instance) ? maybe use a counter (nonce) to identify messages from peers ?
    pub fn broadcast(&self, from: PeerId, msg: Message) {
        let peers = self.peers.lock().unwrap();
        let mut received_messages = self.received_messages.lock().unwrap();

        let from_msg = (from, msg);

        if received_messages.get(&from_msg).is_some() {
            peers.iter().for_each(|(pid, peer)| {
                let mut peer = peer.lock().unwrap();
                if *pid != from {
                    // TODO: Cloning a big message ?
                    let msg = Message::Broadcast(self.local_pid(), Box::new(from_msg.1.clone()));
                    peer.send_and_spawn(msg);
                }
            });

            received_messages.insert(from_msg);
        } else {
            eprintln!("Msg already broadcasted...");
        }
    }

    pub fn insert_peer(&self, peers: &mut Peers, peer: PeerArcMut) {
        let peer_pid = peer.lock().unwrap().pid();
        let mut om = self.orphan_messages.lock().unwrap();

        peers.insert(peer_pid, peer.clone());

        if let Some((ready_tx, _)) = om.remove(&peer_pid) {
            ready_tx
                .send(peer)
                .expect("Peer : Could not send peer to orphan messages.");
        }
    }

    pub fn peer_connection_cancelled(&self, peer_pid: PeerId) {
        match self.peers.lock().unwrap().get(&peer_pid) {
            Some(peer) => {
                let mut peer = peer.lock().unwrap();
                peer.disconnect();
            }
            None => {
                panic!("Shoal : {} : Peer not found in peers.", peer_pid);
            }
        }
    }

    pub fn shutdown(&self) {
        unimplemented!();
    }
}

// ------------------------------------------------------------------
/// # ShoalReadArc
///
/// Simple wrapper around an Arc<RwLock<Shoal>>, so it is easier to use (no `.read().unwrap()`).

#[derive(Clone)]
pub struct ShoalReadArc {
    inner: Arc<RwLock<Shoal>>, // TODO: as tuple : ShoalReadArc(inner) ?
}

impl From<Shoal> for ShoalReadArc {
    fn from(s: Shoal) -> Self {
        ShoalReadArc {
            inner: Arc::new(RwLock::new(s)),
        }
    }
}

impl fmt::Debug for ShoalReadArc {
    fn fmt(&self, writer: &mut fmt::Formatter) -> fmt::Result {
        write!(writer, "{:?}", &*self.lock())
    }
}

impl ShoalReadArc {
    pub fn new(local_pid: PeerId, local_addr: SocketAddr) -> (Self, MpscReceiverMessage) {
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

    pub fn start_clients(&self, runtime: &mut Runtime, addrs: &[SocketAddr]) {
        let shoal = self.lock();
        addrs
            .iter()
            .filter(|peer_addr| **peer_addr != shoal.local_addr)
            .for_each(|peer_addr| {
                let client_future = client::try_connecting_at_interval(self.clone(), *peer_addr);
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

    pub fn swim(&self, runtime: &mut Runtime, addrs: &[SocketAddr]) -> Result<(), Error> {
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

impl fmt::Debug for ShoalReadWeak {
    fn fmt(&self, writer: &mut fmt::Formatter) -> fmt::Result {
        write!(writer, "{:?}", &*self.upgrade().lock())
    }
}

impl ShoalReadWeak {
    fn upgrade(&self) -> ShoalReadArc {
        ShoalReadArc {
            inner: self.inner.upgrade().expect(
                "Could not upgrade Shoal reference, it has been dropped (which should not happen).",
            ),
        }
    }
}
