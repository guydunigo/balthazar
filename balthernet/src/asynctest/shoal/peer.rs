use futures::future::Shared;
use futures::sync::{mpsc, oneshot};
use rand::random;
use tokio::codec::Framed;
use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::timer::Interval;

use std::collections::HashMap;
use std::net::{Shutdown, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

pub type Pid = u32;
pub type ConnVote = u32;

// TODO: async lock?
pub type PeerArcMut = Arc<Mutex<Peer>>;
pub type Peers = HashMap<Pid, PeerArcMut>;
pub type PeersMapArcMut = Arc<Mutex<HashMap<Pid, PeerArcMut>>>;
pub type PeerArcMutOpt = Arc<Mutex<Option<PeerArcMut>>>;
pub type MpscReceiverMessage = mpsc::Receiver<(Pid, Message)>;
pub type MpscSenderMessage = mpsc::Sender<(Pid, Message)>;

/// Interval between ping messages in seconds
const PING_INTERVAL: u64 = 3;

fn vote() -> ConnVote {
    random()
}

/// Don't forget to spawn that...
pub fn send_message(
    socket: TcpStream,
    msg: Message,
) -> impl Future<Item = Framed<TcpStream, MessageCodec>, Error = Error> {
    let framed_sock = Framed::new(socket, MessageCodec::new());

    framed_sock.send(msg.clone()).map_err(move |err| {
        if let Message::Ping = msg {
        } else {
            eprintln!("Error when sending message `{:?}` : `{:?}`.", msg, err);
        }
        Error::from(err)
    })
}

pub fn send_message_and_spawn(socket: TcpStream, msg: Message) {
    let future = send_message(socket, msg).map(|_| ()).map_err(|_| ());
    tokio::spawn(future);
}

pub fn cancel_connection(socket: TcpStream) {
    let future = send_message(socket, Message::ConnectCancel)
        .map_err(Error::from)
        .and_then(|framed_sock| {
            framed_sock
                .get_ref()
                .shutdown(Shutdown::Both)
                .map_err(Error::from)
        })
        .map(|_| ())
        .map_err(|_err| {
            /*
            eprintln!(
                "Listener : Error when sending message `ConnectCancel` : `{:?}`.",
                err
            )
            */
        });

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

    // TODO: send_message in ping ?
    pub fn ping(&mut self) {
        *self = PingStatus::PingSent(Instant::now());
    }

    pub fn pong(&mut self) {
        *self = PingStatus::PongReceived(Instant::now());
    }
}

#[derive(Debug)]
pub enum PeerState {
    NotConnected,
    Connecting(ConnVote),
    Connected(TcpStream),
}

impl Clone for PeerState {
    fn clone(&self) -> Self {
        use self::PeerState::*;
        match self {
            NotConnected => NotConnected,
            Connecting(connvote) => Connecting(*connvote),
            Connected(stream) => Connected(
                stream
                    .try_clone()
                    .expect("Could not clone stream when cloning PeerState"),
            ),
        }
    }
}

#[derive(Debug)]
pub struct Peer {
    // TODO: no `pub` ?
    pid: Pid,
    // shoal: ShoalReadWeak,
    // TODO: Useful to have here a separate tx ?
    shoal_tx: MpscSenderMessage,
    pub addr: SocketAddr,
    pub ping_status: PingStatus,
    pub state: PeerState,
    // TODO: set to false when client socket error
    pub client_connecting: bool,
    // TODO: set to false when listener socket error...
    pub listener_connecting: bool,
    pub ready_rx: Shared<oneshot::Receiver<TcpStream>>,
    ready_tx: Option<oneshot::Sender<TcpStream>>,
}

impl Peer {
    pub fn new(shoal: ShoalReadArc, peer_pid: Pid, addr: SocketAddr) -> Self {
        let (ready_tx, ready_rx) = oneshot::channel();
        Peer {
            pid: peer_pid,
            // shoal: shoal.downgrade(),
            shoal_tx: shoal.lock().tx().clone(),
            addr,
            ping_status: PingStatus::new(),
            state: PeerState::NotConnected,
            client_connecting: false,
            listener_connecting: false,
            ready_tx: Some(ready_tx),
            ready_rx: ready_rx.shared(),
        }
    }

    pub fn _is_connected(&self) -> bool {
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

    pub fn pid(&self) -> Pid {
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
    pub fn connected(&mut self, peer: PeerArcMut, socket: TcpStream) -> Result<(), Error> {
        let socket_clone = socket.try_clone().unwrap();
        // TODO: other checks ?
        self.listener_connecting = false;
        self.client_connecting = false;
        self.state = PeerState::Connected(socket);

        let ready_tx_sent = self.ready_tx.take();

        let sender = match ready_tx_sent {
            Some(sender) => sender,
            None => {
                self.create_oneshot();
                self.ready_tx
                    .take()
                    .expect("The readiness oneshot was just created, it should be Some().")
            }
        };
        sender
            .send(socket_clone)
            .expect("Peer : Couldn't send readiness to peer oneshot.");

        println!("Manager : {} : Connected to : `{}`", self.addr, self.pid);

        self.manage(peer);

        Ok(())
    }

    /// **Important** : peer must be a reference to self.
    pub fn client_connection_acked(&mut self, peer: PeerArcMut, socket: TcpStream) {
        match self.connected(peer, socket) {
            Ok(()) => (),
            _ => unimplemented!(),
        }
    }

    pub fn listener_connection_ack(&mut self, peer: PeerArcMut, socket: TcpStream) {
        match self.connected(peer, socket) {
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

    pub fn client_connection_cancel(&mut self, socket: TcpStream) {
        self.client_connection_cancelled();
        send_message_and_spawn(socket, Message::ConnectCancel);
    }

    pub fn listener_connection_cancel(&mut self, socket: TcpStream) {
        self.listener_connecting = false;
        send_message_and_spawn(socket, Message::ConnectCancel);

        if !self.client_connecting && self.is_connecting() {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn disconnect(&mut self) {
        self.ping_status = PingStatus::NoPingYet;
        self.state = PeerState::NotConnected;
        self.create_oneshot();
        // TODO: disconnect message ?
    }

    /// Don't forget to spawn that...
    // TODO: if send msg error, set to not connected ?
    pub fn send(
        &mut self,
        msg: Message,
    ) -> Box<Future<Item = Framed<TcpStream, MessageCodec>, Error = Error> + Send> {
        if let PeerState::Connected(socket) = &self.state {
            // TODO: unwrap?
            Box::new(send_message(socket.try_clone().unwrap(), msg))
        } else {
            /*
            eprintln!(
                "Can't send `{:?}`, `peer.state` not in `Connected(socket)` !",
                msg
            );
            Box::new(future::err(Error::PeerNotInConnectedState(
                "Can't send message `{:?}`.".to_string(),
            )))
            */
            let peer_pid = self.pid;
            println!(
                "Peer : {} : Setting msg `{:?}` to be sent when peer is ready.",
                peer_pid,
                &format!("{:?}", msg)[..4]
            );

            let future = self
                .ready_rx
                .clone()
                .map_err(|err| Error::OneShotError(err))
                .and_then(|socket| send_message(socket.try_clone().unwrap(), msg))
                .map_err(move |err| {
                    // TODO: Resend the message ?
                    panic!(
                        "Peer : {} : Peer was supposed to be ready but it is not ! {:?}",
                        peer_pid, err
                    );
                });
            Box::new(future)
        }
    }

    pub fn send_and_spawn(&mut self, msg: Message) {
        let future = self.send(msg).map(|_| ()).map_err(|_| ());
        tokio::spawn(future);
    }

    /// **Important** : peer must be a reference to self.
    /// // TODO: ensure that ?
    pub fn manage(&mut self, peer: PeerArcMut) {
        if let PeerState::Connected(socket) = self.state.clone() {
            let socket_clone = socket.try_clone().unwrap();
            let framed_sock = Framed::new(socket, MessageCodec::new());
            let peer_addr = self.addr;
            let peer_clone = peer.clone();
            let shoal_tx = self.shoal_tx.clone();

            let manage_future = framed_sock
                .map_err(Error::from)
                .for_each(move |msg| for_each_message(shoal_tx.clone(), peer.clone(), msg))
                .map_err(move |err| match err {
                    // TODO: println anyway ?
                    Error::ConnectionCancelled | Error::ConnectionEnded => (),
                    Error::PeerNotInConnectedState(_) => (),
                    _ => eprintln!(
                        "Manager : {} : error when receiving a message : {:?}.",
                        peer_addr, err
                    ),
                });

            tokio::spawn(manage_future);

            send_message_and_spawn(socket_clone.try_clone().unwrap(), Message::ConnectAck);

            let ping_future = Interval::new_interval(Duration::from_secs(PING_INTERVAL))
                .inspect_err(move |err| {
                    eprintln!("Manager : {} : Ping error : {:?}", peer_addr, err)
                })
                .map_err(Error::from)
                .and_then(move |_| ping_peer(peer_clone.clone()))
                .for_each(|_| Ok(()))
                .map_err(|_| ());

            tokio::spawn(ping_future);
        }
    }
}

// TODO: Ping if there is already an unanswered Ping ? (in this case, override the Ping time ?)
// TODO: can also use some "last time a message was sent..." to test if necessary, ...
fn ping_peer(peer: PeerArcMut) -> impl Future<Item = (), Error = Error> {
    // TODO: unwrap?
    let mut peer_locked = peer.lock().unwrap();

    let peer_addr = peer_locked.addr;
    let peer_clone = peer.clone();
    let peer_clone_2 = peer.clone();

    peer_locked
        .send(Message::Ping)
        .map(move |_| {
            // TODO: unwrap?
            peer_clone.lock().unwrap().ping();
        })
        .map_err(Error::from)
        .or_else(move |_| {
            // TODO: diagnose and reconnect if necessary...
            println!(
                "Manager : {} : Ping : Failed, setting Peer as disconnected.",
                peer_addr
            );

            // TODO: different way to reconnect ?
            // TODO: unwrap?
            peer_clone_2.lock().unwrap().disconnect();

            Err(Error::PingSendError)
        })
}

// TODO: tx only or shoal directly
fn for_each_message(tx: MpscSenderMessage, peer: PeerArcMut, msg: Message) -> Result<(), Error> {
    // TODO: unwrap?
    // TODO: lock for the whole block ?
    let mut peer = peer.lock().unwrap();

    match msg {
        Message::Ping => {
            let socket = {
                let socket = if let PeerState::Connected(socket) = &peer.state {
                    // TODO: unwrap?
                    socket.try_clone().unwrap()
                } else {
                    eprintln!("Manager : {} : Inconsistent Peer object : a message was received, but `peer.state` is not `Connected(socket)`.", peer.addr);
                    return Err(Error::PeerNotInConnectedState(
                        "Inconsistent Peer object : a message was received.".to_string(),
                    ));
                };

                socket
            };

            send_message_and_spawn(socket, Message::Pong);
        }
        Message::Pong => {
            peer.pong();
            // println!("Manager : {} : received Pong ! It is alive !!!", peer.addr);
        }
        _ => {
            /*
            println!(
                "Manager : {} : received a message (but won't do anything ;) !",
                peer.addr
            );
            */
            let send_future = tx.send((peer.pid, msg)).map(|_| ()).map_err(|_| ());
            tokio::spawn(send_future);
        }
    }

    Ok(())
}
