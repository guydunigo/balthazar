use futures::future::Shared;
use futures::sync::{mpsc, oneshot};
use tokio::io;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use message::{MSetArcMut, Message, Proto, ProtoCodec, M};

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Instant;

use super::Error;
mod client;
mod listener;
mod peer;
pub use self::peer::*;

// TODO: think abuot this size
const SHOAL_MPSC_SIZE: usize = 32;

// TODO: Good for big messages ?
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

// ------------------------------------------------------------------
/// # NotConnectedAction
///
/// This represents what should be done when trying to send a message to an unknown or unconnected peer.
pub enum NotConnectedAction {
    /// The message will be forwarded through the other peers to the destination
    /// (if it can be reached).
    Forward,
    /// The message will be registered to be sent when the peer is directly connected.
    Delay,
    /// The message will not be treated (useful for `Ping` and such)
    Discard,
}

// ------------------------------------------------------------------
/// # Shoal
///
/// This struct stores the data needed across the network (local infos, peers, ...).
/// It provides also methods to interract with all the peers directly (broadcast, ...).
///
/// This structure is not intended to be used as mutable, because there is no need for it :
/// Each of its components are either directly `Arc<Mutex<T>>` or should not be changed.
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
    msgs_received: MSetArcMut,
    orphan_messages: OrphanMsgsMapArcMut,
    nonce_seed: Instant,
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
                msgs_received: Arc::new(Mutex::new(HashSet::new())),
                orphan_messages: Arc::new(Mutex::new(HashMap::new())),
                nonce_seed: Instant::now(),
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

    /// This is used to try if the message was already received.
    ///
    /// Returns `false` if message was already registered.
    /// Returns `true` if message wasn't already registered.
    pub fn try_registering_received_msg(&self, m: &M) -> bool {
        let mut mr = self.msgs_received.lock().unwrap();
        // TODO: clone big message ?
        let m = m.clone();
        mr.insert(m)
    }

    pub fn tx(&self) -> MpscSenderMessage {
        self.tx.clone()
    }

    /// This is a wrapper around `send_to_action` that forwards the message
    /// if the peer is not found or not connected.
    pub fn send_to(&self, peer_pid: PeerId, msg: Message) {
        self.send_to_action(peer_pid, msg, NotConnectedAction::Forward)
    }

    /// This function is based on `send_to_future_action` but directly spawns the future
    /// (in a fire and forget way).
    // TODO: rename to send_to_and_spawn ?
    pub fn send_to_action(&self, peer_pid: PeerId, msg: Message, nc_action: NotConnectedAction) {
        // TODO: do not discard errors ...
        let future = self
            .send_to_future_action(peer_pid, msg, nc_action)
            .map(|_| ())
            .map_err(|_| ());
        tokio::spawn(future);
    }

    /// This is a wrapper around `send_to_future_action` that forwards the message
    /// if the peer is not found or not connected.
    pub fn send_to_future(
        &self,
        peer_pid: PeerId,
        msg: Message,
    ) -> Box<Future<Item = (), Error = Error> + Send> {
        self.send_to_future_action(peer_pid, msg, NotConnectedAction::Forward)
    }

    /// This function sends a message to the given peer.
    /// If the peer is unknown or not connected, `nc_action` will be done.
    ///
    /// It returns a `Future` so that actions can be chained
    /// (sending an ordered list of message for instance)
    // TODO: return an action in `Item` ?
    pub fn send_to_future_action(
        &self,
        peer_pid: PeerId,
        msg: Message,
        nc_action: NotConnectedAction,
    ) -> Box<Future<Item = (), Error = Error> + Send> {
        let peer_opt = {
            let peers = self.peers.lock().unwrap();
            peers.get(&peer_pid).map(|peer| peer.clone())
        };

        match peer_opt {
            Some(peer) => {
                let mut peer = peer.lock().unwrap();
                Box::new(
                    peer.send_action(Proto::Direct(M::new(self.local_pid(), msg)), nc_action)
                        .map(|_| ()),
                )
            }
            None => {
                match nc_action {
                    NotConnectedAction::Forward => {
                        // println!("Shoal : Peer `{}` is not a direct peer, sending a `ForwardTo` of message `{}` to other peers...", peer_pid, msg);

                        self.forward_msg(peer_pid, msg);
                        // TODO: future that resolves only when receiving some ack from target ?
                        Box::new(future::ok(()))
                    }
                    NotConnectedAction::Delay => {
                        // println!("Shoal : Setting msg `{}` to be sent when peer `{}` is created.", msg, peer_pid);

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

                        let local_pid = self.local_pid();
                        let future = ready_rx.map_err(|err| Error::OneShotError(err)).and_then(
                            move |peer| {
                                peer.lock()
                                    .unwrap()
                                    .send_action(Proto::Direct(M::new(local_pid, msg)), nc_action)
                            },
                        );

                        Box::new(future)
                    }
                    NotConnectedAction::Discard => Box::new(future::ok(())),
                }
            }
        }
    }

    pub fn broadcast_msg(&self, msg: Message) {
        self.broadcast(Vec::new(), M::new(self.local_pid(), msg))
    }

    /// This function send `msg` to all connected peers only if
    /// - it doesn't appear in the route list of the `Broadcast` message.
    /// (i.e. if it wasn't already broadcasted via this peer)
    /// - the peer is not already in the route list
    /// (if it hasn't already gone through this peer)

    // TODO: Discard if already broadcasted
    pub fn broadcast(&self, mut route_list: Vec<PeerId>, m: M) {
        let peers = self.peers.lock().unwrap();

        // TODO: better way to find ?
        if route_list
            .iter()
            .find(|pid| **pid == self.local_pid)
            .is_none()
        {
            route_list.push(self.local_pid());
            peers.iter().for_each(|(pid, peer)| {
                let mut peer = peer.lock().unwrap();
                // TODO: check if connected ?
                if peer.is_connected() {
                    if route_list.iter().find(|p| **p == *pid).is_none() {
                        // TODO: Cloning a big message and a big route_list ?
                        let pkt = Proto::Broadcast(route_list.clone(), m.clone());
                        peer.send_and_spawn_action(pkt, NotConnectedAction::Discard);
                    } else {
                        // eprintln!("Message `{}` already gone through peer `{}`, {:?}.", msg, pid, route_list);
                    }
                }
            });
        } else {
            // eprintln!("Message `{}` already broadcasted, {:?}.", msg, route_list);
        }
    }

    pub fn forward_msg(&self, to: PeerId, msg: Message) {
        self.forward(to, Vec::new(), M::new(self.local_pid(), msg))
    }

    // TODO: Discard if already forwarded
    // TODO: send Found/NotFound ?
    pub fn forward(&self, to: PeerId, mut route_list: Vec<PeerId>, m: M) {
        let peers = self.peers.lock().unwrap();

        if to == self.local_pid() {
            if route_list.len() == 0 {
                eprintln!(
                "The route list is empty, which can mean : 1. The Shoal is trying to send the message to itself or 2. a `ForwardTo` with no sender has been sent."
            );
            }

            // TODO: take care of the message: directly forward to upper layer ?
            // TODO: save the route to the sender peer ? and re-use it next time ?
            let future = self
                .tx()
                .send((m.from_pid, m.msg))
                .map(|_| ())
                .map_err(|_| ());
            tokio::spawn(future);
        } else if let Some((_, peer)) = peers.iter().find(|(pid, _)| **pid == to) {
            let mut peer = peer.lock().unwrap();

            route_list.push(self.local_pid());

            let pkt = Proto::ForwardTo(to, route_list.clone(), m);
            peer.send_and_spawn_action(pkt, NotConnectedAction::Discard);
        // TODO: better way to find ?
        } else if route_list
            .iter()
            .find(|pid| **pid == self.local_pid)
            .is_none()
        {
            route_list.push(self.local_pid());

            peers.iter().for_each(|(pid, peer)| {
                let mut peer = peer.lock().unwrap();
                if peer.is_connected() {
                    if route_list.iter().find(|p| **p == *pid).is_none() {
                        // TODO: Cloning a big message and a big route_list ?
                        let pkt = Proto::ForwardTo(to, route_list.clone(), m.clone());
                        peer.send_and_spawn_action(pkt, NotConnectedAction::Discard);
                    } else {
                        // eprintln!("Message `{}` already gone through peer `{}`, {:?}", msg, pid, route_list);
                    }
                } else {
                    // eprintln!("Peer `{}` not connected...", pid);
                }
            });
        } else {
            // eprintln!("Message `{}` already forwarded, {:?}.", msg, route_list);
        }
    }

    // TODO: forward with prefered route

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
