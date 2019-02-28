use futures::future::Shared;
use futures::sync::{mpsc, oneshot};
use rand::random;
use tokio::codec::{FramedRead, FramedWrite};
use tokio::io::ReadHalf;
use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::timer::Interval;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

pub type PeerId = u32;
pub type ConnVote = u32;

// TODO: async lock?
pub type PeerArcMut = Arc<Mutex<Peer>>;
pub type Peers = HashMap<PeerId, PeerArcMut>;
pub type PeersMapArcMut = Arc<Mutex<HashMap<PeerId, PeerArcMut>>>;
pub type PeerArcMutOpt = Arc<Mutex<Option<PeerArcMut>>>;
pub type MpscReceiverMessage = mpsc::Receiver<(PeerId, Message)>;
pub type MpscSenderMessage = mpsc::Sender<(PeerId, Message)>;

/// Interval between ping packets in seconds
const PING_INTERVAL: u64 = 3;
/// Size of the queue for sending messages
const MPSC_TX_SIZE: usize = 9;

fn vote() -> ConnVote {
    // TODO: local node id ?
    random()
}

pub fn send_packet(
    mpsc_tx: mpsc::Sender<Proto>,
    pkt: Proto,
) -> impl Future<Item = mpsc::Sender<Proto>, Error = Error> {
    mpsc_tx.send(pkt).map_err(move |err| {
        if let Proto::Ping = pkt {
        } else {
            eprintln!("Error when sending packet `{}` : `{:?}`.", pkt, err);
        }
        Error::from(err)
    })
}

pub fn send_packet_and_spawn(mpsc_tx: mpsc::Sender<Proto>, pkt: Proto) {
    let future = send_packet(mpsc_tx, pkt).map(|_| ()).map_err(|_| ());
    tokio::spawn(future);
}

pub fn cancel_connection(mpsc_tx: mpsc::Sender<Proto>) {
    let future = mpsc_tx
        .send(Proto::ConnectCancel)
        .map_err(Error::from)
        .map_err(|_err| {
            /*
            eprintln!(
                "Listener : Error when sending packet `ConnectCancel` : `{:?}`.",
                err
            )
            */
        })
        .and_then(|_| mpsc_tx.close().map_err(|_| ()).map(|_| ()));

    tokio::spawn(future);
}

#[derive(Debug, Clone, Copy)]
pub enum PingStatus {
    PingSent(Instant),
    PongReceived(Instant),
    NoPingYet,
}

impl PingStatus {
    pub fn new() -> Self {
        PingStatus::NoPingYet
    }

    pub fn is_ping_sent(&self) -> bool {
        if let PingStatus::PingSent(_) = self {
            true
        } else {
            false
        }
    }

    // TODO: send_packet in ping ?
    pub fn ping(&mut self) {
        *self = PingStatus::PingSent(Instant::now());
    }

    pub fn pong(&mut self) {
        *self = PingStatus::PongReceived(Instant::now());
    }
}

#[derive(Debug, Clone)]
pub enum PeerState {
    NotConnected,
    Connecting(ConnVote),
    Connected(mpsc::Sender<Proto>),
}

#[derive(Debug)]
// TODO: no `pub` ?
pub struct Peer {
    pid: PeerId,
    shoal: ShoalReadWeak,
    // TODO: maybe known connection ports (optional)
    pub addr: SocketAddr,
    pub ping_status: PingStatus,
    pub state: PeerState,
    // TODO: set to false when client socket error
    pub client_connecting: bool,
    // TODO: set to false when listener socket error...
    pub listener_connecting: bool,
    // TODO: generics AsyncWrite/Read
    pub ready_rx: Shared<oneshot::Receiver<mpsc::Sender<Proto>>>,
    ready_tx: Option<oneshot::Sender<mpsc::Sender<Proto>>>,
}

impl Peer {
    pub fn new(shoal: ShoalReadArc, peer_pid: PeerId, addr: SocketAddr) -> Self {
        let (ready_tx, ready_rx) = oneshot::channel();
        Peer {
            pid: peer_pid,
            shoal: shoal.downgrade(),
            addr,
            ping_status: PingStatus::new(),
            state: PeerState::NotConnected,
            client_connecting: false,
            listener_connecting: false,
            ready_tx: Some(ready_tx),
            ready_rx: ready_rx.shared(),
        }
    }

    // TODO: is state directly
    pub fn is_connected(&self) -> bool {
        if let PeerState::Connected(_) = self.state {
            true
        } else {
            false
        }
    }

    pub fn is_connecting(&self) -> bool {
        if let PeerState::Connecting(_) = self.state {
            true
        } else {
            false
        }
    }

    pub fn _is_ping_sent(&self) -> bool {
        self.ping_status.is_ping_sent()
    }

    pub fn pid(&self) -> PeerId {
        self.pid
    }

    pub fn ping(&mut self) {
        self.ping_status.ping()
    }

    pub fn pong(&mut self) {
        self.ping_status.pong()
    }

    pub fn to_connecting(&mut self) -> ConnVote {
        let local_vote = vote();
        self.state = PeerState::Connecting(local_vote);
        local_vote
    }

    pub fn listener_to_connecting(&mut self) -> ConnVote {
        self.listener_connecting = true;
        self.to_connecting()
    }

    pub fn client_to_connecting(&mut self) -> ConnVote {
        self.client_connecting = true;
        self.to_connecting()
    }

    fn create_oneshot(&mut self) {
        let (ready_tx, ready_rx) = oneshot::channel();
        self.ready_tx = Some(ready_tx);
        self.ready_rx = ready_rx.shared();
    }

    // TODO: return Result ?
    /// **Important** : peer must be a reference to self.
    /// TODO: to a wrapper like for Shoal ?
    pub fn connected(
        &mut self,
        peer: PeerArcMut,
        mpsc_tx: mpsc::Sender<Proto>,
    ) -> Result<(), Error> {
        // TODO: other checks ?
        self.listener_connecting = false;
        self.client_connecting = false;
        self.state = PeerState::Connected(mpsc_tx);

        let ready_tx_sent = self.ready_tx.take();

        let sender = match ready_tx_sent {
            Some(sender) => sender,
            None => {
                self.create_oneshot();
                self.ready_tx
                    .take()
                    .expect("The readiness oneshot has just been created, it should be Some().")
            }
        };
        sender
            .send(mpsc_tx)
            .expect("Peer : Couldn't send readiness to peer oneshot.");

        println!("Manager : {} : Connected to : `{}`", self.pid, self.addr);

        unimplemented!();
        // TODO: self.manage(peer, socket_rx);

        Ok(())
    }

    /// **Important** : peer must be a reference to self.
    pub fn client_connection_acked(&mut self, peer: PeerArcMut, mpsc_tx: mpsc::Sender<Proto>) {
        match self.connected(peer, mpsc_tx) {
            Ok(()) => (),
            _ => unimplemented!(),
        }
    }

    pub fn listener_connection_ack(&mut self, peer: PeerArcMut, mpsc_tx: mpsc::Sender<Proto>) {
        match self.connected(peer, mpsc_tx) {
            Ok(()) => (),
            _ => unimplemented!(),
        }
    }

    pub fn client_connection_cancelled(&mut self) {
        self.client_connecting = false;

        if !self.listener_connecting && self.is_connecting() {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn client_connection_cancel(&mut self, mpsc_tx: mpsc::Sender<Proto>) {
        self.client_connection_cancelled();

        send_packet_and_spawn(mpsc_tx, Proto::ConnectCancel);
    }

    pub fn listener_connection_cancel(&mut self, mpsc_tx: mpsc::Sender<Proto>) {
        self.listener_connecting = false;

        send_packet_and_spawn(mpsc_tx, Proto::ConnectCancel);

        if !self.client_connecting && self.is_connecting() {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn connected_cancel(&mut self) {
        // TODO: Wait ?
        self.send_action(Proto::ConnectCancel, NotConnectedAction::Discard)
            .map(|_| ())
            .wait()
            .unwrap_or_default();
        self.disconnect();
    }

    pub fn disconnect(&mut self) {
        self.ping_status = PingStatus::NoPingYet;
        self.client_connecting = false;
        self.listener_connecting = false;
        self.create_oneshot();
        // TODO: disconnect packet ?

        // TODO: What about ReadHalf ?

        self.state = PeerState::NotConnected;
    }

    // TODO: if send pkt error, set to not connected ?
    /// This function sends a packet to the peer.
    /// If the peer is unknown or not connected, `nc_action` will be done.
    ///
    /// It returns a `Future` so that actions can be chained
    /// (sending an ordered list of packet for instance)
    // TODO: return an action in `Item` ?
    pub fn send_action(
        &mut self,
        pkt: Proto,
        nc_action: NotConnectedAction,
    ) -> Box<Future<Item = (), Error = Error> + Send> {
        if let PeerState::Connected(mpsc_tx) = &self.state {
            // TODO: unwrap?
            Box::new(send_packet(mpsc_tx.clone(), pkt).map(|_| ()))
        // TODO: delete this branch:
        } else if let Proto::Ping = pkt {
            // Don't register Pings...
            Box::new(future::err(Error::PingSendError))
        } else {
            match nc_action {
                NotConnectedAction::Forward => {
                    // TODO: Shouldn't arrive here ?
                    unimplemented!();
                }
                NotConnectedAction::Delay => {
                    let peer_pid = self.pid;
                    println!(
                        "Peer : {} : Setting pkt `{}` to be sent when peer is ready.",
                        peer_pid, pkt
                    );

                    let future = self
                        .ready_rx
                        .clone()
                        .map_err(|err| Error::OneShotError(err))
                        .and_then(|mpsc_tx| send_packet(*mpsc_tx, pkt))
                        .map(|_| ())
                        .map_err(move |err| {
                            // TODO: Resend the packet ? Don't panic ?
                            eprintln!(
                                "Peer : {} : Peer was supposed to be ready but it is not : `{:?}`",
                                peer_pid, err
                            );
                            err
                        });
                    Box::new(future)
                }
                NotConnectedAction::Discard => Box::new(future::ok(())),
            }
        }
    }

    /// This function is based on `send` but directly spawns the future
    /// (in a fire and forget way).
    pub fn send_and_spawn_action(&mut self, pkt: Proto, nc_action: NotConnectedAction) {
        let future = self.send_action(pkt, nc_action).map(|_| ()).map_err(|_| ());
        tokio::spawn(future);
    }

    /// **Important** : peer must be a reference to self.
    /// // TODO: ensure that ?
    // TODO: manage should be called from the client or listener loop
    // instead of creating another one
    pub fn manage(&mut self, peer: PeerArcMut, socket_rx: ReadHalf<TcpStream>) {
        if let PeerState::Connected(mpsc_tx) = self.state.clone() {
            let framed_sock = FramedRead::new(socket_rx, ProtoCodec::new(Some(self.pid())));
            let peer_pid = self.pid();
            let peer_clone = peer.clone();
            let shoal = self.shoal.clone();

            mpsc_tx.send(Proto::ConnectAck);

            {
                // ping
                let ping_future = Interval::new_interval(Duration::from_secs(PING_INTERVAL))
                    .map_err(Error::from)
                    .and_then(move |_| ping_peer(peer_clone.clone()))
                    .for_each(|_| Ok(()))
                    .map_err(|_| ());
                tokio::spawn(ping_future);
            }

            let manage_future = framed_sock
                .map_err(Error::from)
                .for_each(move |pkt| {
                    for_each_packet(shoal.upgrade(), &mut *peer.lock().unwrap(), pkt)
                })
                .map_err(move |err| match err {
                    // TODO: println anyway ?
                    Error::ConnectionCancelled | Error::ConnectionEnded => (),
                    Error::PeerNotInConnectedState(_) => (),
                    _ => eprintln!(
                        "Manager : {} : error when receiving a packet : {:?}.",
                        peer_pid, err
                    ),
                });

            tokio::spawn(manage_future);
        }
    }

    pub fn handle_pkt(&mut self, pkt: Proto) -> Result<(), Error> {
        for_each_packet(self.shoal.upgrade(), self, pkt)
    }
}

// TODO: Ping if there is already an unanswered Ping ? (in this case, override the Ping time ?)
// TODO: can also use some "last time a packet was sent..." to test if necessary, ...
fn ping_peer(peer: PeerArcMut) -> Box<Future<Item = (), Error = Error> + Send> {
    let mut peer_locked = peer.lock().unwrap();

    if peer_locked.is_connected() {
        let peer_pid = peer_locked.pid;
        let peer_clone = peer.clone();
        let peer_clone_2 = peer.clone();

        let future = peer_locked
            .send_action(Proto::Ping, NotConnectedAction::Discard)
            .map(move |_| {
                peer_clone.lock().unwrap().ping();
            })
            .map_err(Error::from)
            .or_else(move |_| {
                // TODO: diagnose and reconnect if necessary...
                println!(
                    "Manager : {} : Ping : Failed, setting Peer as disconnected.",
                    peer_pid
                );

                peer_clone_2.lock().unwrap().disconnect();

                Err(Error::PingSendError)
            });
        Box::new(future)
    } else {
        Box::new(future::err(Error::PingSendError))
    }
}

fn for_each_packet(shoal: ShoalReadArc, peer: &mut Peer, pkt: Proto) -> Result<(), Error> {
    let shoal_clone = shoal.clone();
    let shoal = shoal.lock();
    match pkt {
        Proto::Ping => {
            let mpsc_tx = if let PeerState::Connected(mpsc_tx) = &peer.state {
                mpsc_tx
            } else {
                eprintln!("Manager : {} : Inconsistent Peer object : a packet was received, but `peer.state` is not `Connected(socket)`.", peer.addr);
                return Err(Error::PeerNotInConnectedState(
                    "Inconsistent Peer object : a packet was received.".to_string(),
                ));
            };

            send_packet_and_spawn(mpsc_tx.clone(), Proto::Pong);
        }
        Proto::Pong => {
            peer.pong();
            // println!("Manager : {} : received Pong ! It is alive !!!", peer.pid);
        }
        Proto::Broadcast(route_list, m) => {
            if shoal.try_registering_received_msg(&m) {
                // TODO: cloning big msgs ?
                let m_clone = m.clone();
                // TODO: better way to avoid the lock ?
                let future = future::ok(()).and_then(move |_| {
                    shoal_clone.lock().broadcast(route_list, m);
                    Ok(())
                });
                tokio::spawn(future);

                let send_future = shoal
                    .tx()
                    .send((m_clone.from_pid, m_clone.msg))
                    .map(|_| ())
                    .map_err(|_| ());
                tokio::spawn(send_future);
            }
        }
        Proto::ForwardTo(to, route_list, m) => {
            if shoal.try_registering_received_msg(&m) {
                // TODO: better way to avoid the lock ?
                let future = future::ok(()).and_then(move |_| {
                    shoal_clone.lock().forward(to, route_list, m);
                    Ok(())
                });
                tokio::spawn(future);
            }
        }
        Proto::Direct(m) => {
            // TODO: check if peer.pid() == m.from_pid ?
            if shoal.try_registering_received_msg(&m) {
                /*
                println!(
                    "Manager : {} : received a packet, sending it upper levels.",
                    peer.pid
                );
                */
                let send_future = shoal
                    .tx()
                    .send((m.from_pid, m.msg))
                    .map(|_| ())
                    .map_err(|_| ());
                tokio::spawn(send_future);
            }
        }
        _ => {
            eprintln!(
                "Manager : {} : Unhandled `Proto` packet : `{}`",
                peer.pid, pkt
            );
        }
    }

    Ok(())
}

// TODO: don't shadow error
// TODO: 'static :/
pub fn write_to_mpsc<W: AsyncWrite + Send + 'static>(tx: W) -> mpsc::Sender<Proto> {
    let (mpsc_tx, mpsc_rx) = mpsc::channel(MPSC_TX_SIZE);

    let framed_sock = FramedWrite::new(tx, ProtoCodec::new(None));
    let framed_future = framed_sock.send_all(mpsc_rx).map(|_| ()).map_err(|_| ());
    tokio::spawn(framed_future);

    mpsc_tx
}
