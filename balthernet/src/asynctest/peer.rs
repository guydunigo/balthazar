use rand::random;
use tokio::codec::Framed;
use tokio::net::TcpStream;
use tokio::prelude::*;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::{Error, MessageCodec};
use balthmessage as message;
use balthmessage::Message;

pub type Pid = u32;
pub type ConnVote = u32;

// TODO: beware of deadlocking a peer ?
// TODO: async lock?
pub type PeerArcMut = Arc<Mutex<Peer>>;
pub type PeersMapArcMut = Arc<Mutex<HashMap<Pid, PeerArcMut>>>;

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

    framed_sock.send(msg).map_err(move |err| {
        eprintln!("Error when sending message : `{:?}`.", err);
        Error::from(err)
    })
}

pub fn send_message_and_spawn(socket: TcpStream, msg: Message) {
    let future = send_message(socket, msg).map(|_| ()).map_err(|_| ());
    tokio::spawn(future);
}

#[derive(Debug)]
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

    pub fn ping(&mut self) {
        *self = PingStatus::PingSent(Instant::now());
    }

    pub fn pong(&mut self) {
        *self = PingStatus::PongReceived(Instant::now());
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PeerState {
    // TODO: ping uses this values?
    NotConnected,
    Connecting(ConnVote),
    Connected,
}

#[derive(Debug)]
pub struct Peer {
    // TODO: no `pub` ?
    pid: Option<Pid>,
    pub addr: SocketAddr,
    pub socket: Option<TcpStream>,
    pub ping_status: PingStatus,
    // TODO: remove this todo if connection is done
    pub state: PeerState,
    pub client_connecting: bool,
    pub listener_connecting: bool,
}

impl Peer {
    pub fn new(addr: SocketAddr) -> Self {
        Peer {
            // TODO: do something with pid...
            pid: None,
            addr,
            socket: None,
            ping_status: PingStatus::new(),
            // TODO: remove this todo if connection is done
            state: PeerState::NotConnected,
            client_connecting: false,
            listener_connecting: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        if let PeerState::Connected = self.state {
            true
        } else {
            false
        }
    }

    pub fn is_ping_sent(&self) -> bool {
        self.ping_status.is_ping_sent()
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
    pub fn connected(&mut self, socket: TcpStream) -> Result<(), Error> {
        if let Some(pid) = self.pid {
            // TODO: other checks ?
            self.listener_connecting = false;
            self.client_connecting = false;
            self.state = PeerState::Connected;
            self.socket = Some(socket);

            // TODO: make sure this is the last ref to socket?
            // TODO: start listening thread...
            println!("Connected to : `{}`", pid);

            Ok(())
        } else {
            Err(Error::PidMissing)
        }
    }

    // TODO: too close names ? `{listener,client}_connection` ?
    pub fn client_connection_acked(&mut self, socket: TcpStream) {
        match self.connected(socket) {
            Ok(()) => (),
            Err(Error::PidMissing) => {
                eprintln!("Received `Message::ConnectAck` but pid is missing.")
            }
            _ => unimplemented!(),
        }
    }

    pub fn listener_connection_ack(&mut self, socket: TcpStream) {
        match self.connected(socket) {
            Ok(()) => (),
            Err(Error::PidMissing) => {
                eprintln!("Can't send `Message::ConnectAck` : pid is missing.")
            }
            _ => unimplemented!(),
        }
        self.send_and_spawn(Message::ConnectAck);
    }

    pub fn client_connection_cancelled(&mut self) {
        self.client_connecting = false;

        if !self.listener_connecting {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn listener_connection_cancel(&mut self) {
        self.listener_connecting = false;
        self.send_and_spawn(Message::ConnectCancel);

        if !self.client_connecting {
            self.state = PeerState::NotConnected;
        }
    }

    pub fn disconnect(&mut self) {
        self.socket = None;
        self.ping_status = PingStatus::NoPingYet;
        self.state = PeerState::NotConnected;
        // TODO: disconnect message ?
    }

    // TODO: return Result ?
    pub fn set_pid(&mut self, pid: Pid) {
        if let Some(present_pid) = self.pid {
            eprintln!(
                "Attempting to write pid `{}`, but pid is already set `{}`.",
                pid, present_pid
            );
        } else {
            self.pid = Some(pid);
        }
    }

    /// Don't forget to spawn that...
    pub fn send(
        &mut self,
        msg: Message,
    ) -> impl Future<Item = Framed<TcpStream, MessageCodec>, Error = Error> {
        if let Some(socket) = &self.socket {
            // TODO: unwrap?
            send_message(socket.try_clone().unwrap(), msg)
        } else {
            panic!("Can't send `{:?}`, no socket present!", msg);
        }
    }

    pub fn send_and_spawn(&mut self, msg: Message) {
        let future = self.send(msg).map(|_| ()).map_err(|_| ());
        tokio::spawn(future);
    }
}

pub fn for_each_message_connected(
    addr: SocketAddr,
    peer: PeerArcMut,
    msg: &Message,
) -> Result<(), message::Error> {
    match msg {
        Message::Ping => {
            // TODO: unwrap?
            let (addr, socket) = {
                let peer = peer.lock().unwrap();
                let socket = if let Some(socket) = &peer.socket {
                    // TODO: unwrap?
                    socket.try_clone().unwrap()
                } else {
                    panic!("Inconsistent Peer object : a message was received, but `peer.socket` is `None` (and `peer.state` is `PeerState::Connected`).");
                };

                (peer.addr, socket)
            };

            send_message_and_spawn(socket, Message::Ping);
        }
        Message::Pong => {
            // TODO: unwrap?
            peer.lock().unwrap().pong();
            // println!("{} : received Pong ! It is alive !!!", addr);
        }
        _ => println!("{} : received a message (but won't do anything ;) !", addr),
    }

    Ok(())
}
