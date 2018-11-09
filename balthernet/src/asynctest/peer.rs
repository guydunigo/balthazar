use tokio::net::TcpStream;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub type Pid = u32;
pub type RandVote = u32;

// TODO: beware of deadlocking a peer ?
// TODO: async lock?
pub type PeerArcMut = Arc<Mutex<Peer>>;
pub type PeersMapArcMut = Arc<Mutex<HashMap<Pid, PeerArcMut>>>;

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
    Connecting(RandVote),
    Connected,
}

#[derive(Debug)]
pub struct Peer {
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

    pub fn remove_socket(&mut self) {
        self.socket = None;
        self.ping_status = PingStatus::NoPingYet;
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

    pub fn client_connect(&mut self, vote: RandVote) {
        self.client_connecting = true;
        self.state = PeerState::Connecting(vote);
    }

    // TODO: return Result ?
    pub fn client_connected(&mut self, socket: TcpStream) {
        if let Some(pid) = self.pid {
            self.client_connecting = false;
            self.state = PeerState::Connected;
            self.socket = Some(socket);

            println!("Connected to : `{}`", pid);
        } else {
            eprintln!("Received `Message::ConnectAck` but pid is missing.");
        }
    }

    pub fn client_cancel_connection(&mut self) {
        self.state = PeerState::NotConnected;
        self.client_connecting = false;
        self.socket = None;
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
}
