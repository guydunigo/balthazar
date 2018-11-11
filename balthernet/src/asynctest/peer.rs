use rand::random;
use tokio::codec::Framed;
use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::timer::Interval;

use std::collections::HashMap;
use std::net::{Shutdown, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Error, MessageCodec};
use balthmessage::Message;

pub type Pid = u32;
pub type ConnVote = u32;

// TODO: async lock?
pub type PeerArcMut = Arc<Mutex<Peer>>;
pub type PeersMapArcMut = Arc<Mutex<HashMap<Pid, PeerArcMut>>>;
pub type PeerArcMutOpt = Arc<Mutex<Option<PeerArcMut>>>;

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
    // TODO: unwrap?
    let framed_sock = Framed::new(socket, MessageCodec::new());

    framed_sock.send(msg.clone()).map_err(move |err| {
        eprintln!("Error when sending message `{:?}` : `{:?}`.", msg, err);
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
    // TODO: ping uses these values?
    NotConnected,
    Connecting(ConnVote),
    // TODO: Connected(TcpStream)
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
    peer_pid: Pid,
    local_pid: Pid,
    peers: PeersMapArcMut,
    pub addr: SocketAddr,
    pub ping_status: PingStatus,
    pub state: PeerState,
    // TODO: set to false when client socket error
    pub client_connecting: bool,
    // TODO: set to false when listener socket error...
    pub listener_connecting: bool,
}

impl Peer {
    pub fn new(local_pid: Pid, peer_pid: Pid, addr: SocketAddr, peers: PeersMapArcMut) -> Self {
        Peer {
            peer_pid,
            local_pid,
            addr,
            peers,
            ping_status: PingStatus::new(),
            state: PeerState::NotConnected,
            client_connecting: false,
            listener_connecting: false,
        }
    }

    pub fn _is_connected(&self) -> bool {
        if let PeerState::Connected(_) = self.state {
            true
        } else {
            false
        }
    }

    pub fn _is_ping_sent(&self) -> bool {
        self.ping_status.is_ping_sent()
    }

    pub fn peer_pid(&self) -> Pid {
        self.peer_pid
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

    // TODO: return Result ?
    /// **Important** : peer must be a reference to self.
    pub fn connected(&mut self, peer: PeerArcMut, socket: TcpStream) -> Result<(), Error> {
        // TODO: other checks ?
        self.listener_connecting = false;
        self.client_connecting = false;
        self.state = PeerState::Connected(socket);

        println!(
            "Manager : {} : Connected to : `{}`",
            self.addr, self.peer_pid
        );

        self.manage(peer);

        Ok(())
    }

    // TODO: too close names ? `{listener,client}_connection` ?
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

        if !self.listener_connecting {
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

        if !self.client_connecting {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn disconnect(&mut self) {
        self.ping_status = PingStatus::NoPingYet;
        self.state = PeerState::NotConnected;
        // TODO: disconnect message ?
    }

    /// Don't forget to spawn that...
    pub fn send(
        &mut self,
        msg: Message,
    ) -> Box<Future<Item = Framed<TcpStream, MessageCodec>, Error = Error> + Send> {
        if let PeerState::Connected(socket) = &self.state {
            // TODO: unwrap?
            Box::new(send_message(socket.try_clone().unwrap(), msg))
        } else {
            eprintln!(
                "Can't send `{:?}`, `peer.state` not in `Connected(socket)` !",
                msg
            );
            Box::new(future::err(Error::PeerNotInConnectedState(
                "Can't send message `{:?}`.".to_string(),
            )))
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
            let framed_sock = Framed::new(socket, MessageCodec::new());
            let peer_addr = self.addr;
            let peer_clone = peer.clone();

            let manage_future = framed_sock
                .map_err(Error::from)
                .for_each(move |msg| for_each_message(peer.clone(), &msg))
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

            self.send_and_spawn(Message::ConnectAck);

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

fn for_each_message(peer: PeerArcMut, msg: &Message) -> Result<(), Error> {
    let mut peer = peer.lock().unwrap();

    match msg {
        Message::Ping => {
            // TODO: unwrap?
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
        _ => println!(
            "Manager : {} : received a message (but won't do anything ;) !",
            peer.addr
        ),
    }

    Ok(())
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
                "Manager : {} : Ping : Failed, triggering reconnection...",
                peer_addr
            );

            // TODO: different way to reconnect ?
            // TODO: unwrap?
            peer_clone_2.lock().unwrap().disconnect();

            Err(Error::PingSendError)
        })
}
