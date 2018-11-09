use rand::random;
use tokio::codec::Framed;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::timer::Interval;

use balthmessage::Message;

use std::boxed::Box;
use std::collections::HashMap;
use std::fs::File;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

pub mod message_codec;
type MessageCodec = message_codec::MessageCodec;

type Pid = u32;
type RandVote = u32;

/// Interval between connections tries in seconds
const CONNECTION_INTERVAL: u64 = 10;

// TODO: beware of deadlocking a peer ?
// TODO: async lock?
type PeerArcMut = Arc<Mutex<Peer>>;
type PeersMapArcMut = Arc<Mutex<HashMap<Pid, PeerArcMut>>>;

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

    pub fn is_already_connected(&self) -> bool {
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

fn connect_to_peer(
    pid: Pid,
    peer: PeerArcMut,
    peers: PeersMapArcMut,
) -> impl Future<Item = (), Error = Error> {
    // TODO: unwrap?
    let addr = peer.lock().unwrap().addr;
    let peer2 = peer.clone();
    let peer3 = peer.clone();

    TcpStream::connect(&addr)
        .map_err(Error::from)
        .and_then(move |socket| {
            let framed_sock = Framed::new(socket, MessageCodec::new());
            let vote: RandVote = random();

            {
                // TODO: unwrap?
                let mut peer = peer2.lock().unwrap();
                peer.client_connect(vote);
            }

            framed_sock.send(Message::Connect(pid, vote)).map_err(Error::from)
        })
        // TODO: Move to separate function
        .and_then(move |framed_sock| {
            let socket = framed_sock.get_ref().try_clone().unwrap();

            let manager = framed_sock
                .for_each(move |msg| {
                    let state = peer3.lock().unwrap().state;

                    match state {
                        PeerState::Connecting(_) => match msg {
                            Message::ConnectReceived(pid) => {
                                peer3.lock().unwrap().set_pid(pid);

                                // TODO: double lock...
                                // TODO: can this lock kill the mutex ?
                                if let Some(_) = peers.lock().unwrap().get(&pid) {
                                    // TODO: check that there can be a connectAck, cancel listener
                                    // procedure if necessary...
                                    unimplemented!("There is already a peer in peers");
                                } else {
                                    peers.lock().unwrap().insert(pid, peer3.clone());
                                }
                            },
                            Message::ConnectAck => {
                                // TODO: unwrap?
                                let mut peer = peer3.lock().unwrap();
                                // TODO: cloning twice the socket ?
                                peer.client_connected(socket.try_clone().unwrap());
                            },
                            Message::ConnectCancel => {
                                peer3.lock().unwrap().client_cancel_connection();
                                // TODO: kill this loop...
                                unimplemented!();
                            },
                            _ => unimplemented!(),
                        },
                        PeerState::Connected => match msg {
                            Message::Ping => {
                                // TODO: unwrap?
                                let (addr, socket) = {
                                    let peer = peer3.lock().unwrap();
                                    let socket = if let Some(socket) = &peer.socket {
                                        // TODO: unwrap?
                                        socket.try_clone().unwrap()
                                    } else {
                                        panic!("Inconsistent Peer object : a message was received, but `peer.socket` is `None` (and `peer.state` is `PeerState::Connected`).");
                                    };

                                    (peer.addr, socket)
                                };

                                let framed_sock = Framed::new(socket, MessageCodec::new());
                                let send_future = framed_sock.send(Message::Pong).map(|_| ()).map_err(move |err| eprintln!("{} : Could no send `Pong` : {:?}", addr, err));

                                tokio::spawn(send_future);
                            },
                            Message::Pong => {
                                // TODO: unwrap?
                                peer3.lock().unwrap().pong();
                                // println!("{} : received Pong ! It is alive !!!", addr);
                            }
                            _ => println!("{} : received a message (no action linked) !", addr),
                        },
                        PeerState::NotConnected => panic!("Inconsistent Peer object : a message was received, but `peer.state` is `PeerState::NotConnected`."),
                    }
                    Ok(())
                })
            .map_err(move |err| {
                eprintln!("{} : error when receiving a message : {:?}.", addr, err)
            });

        tokio::spawn(manager);

        Ok(())
    })
}

// TODO: Ping if there is already an unanswered Ping ? (in this case, override the Ping time ?)
fn ping_peer(peer: PeerArcMut) -> impl Future<Item = (), Error = Error> {
    let peer2 = peer.clone();

    let (addr, socket) = {
        let peer = peer.lock().unwrap();

        let socket = if let Some(socket) = &peer.socket {
            // TODO: unwrap?
            socket.try_clone().unwrap()
        } else {
            panic!("Peer has no Socket, can't Ping !");
        };

        (peer.addr, socket)
    };

    Framed::new(socket, MessageCodec::new())
        .send(Message::Ping)
        // TODO: is this map useful ?
        .map(move |_| {
            // TODO: unwrap?
            peer.lock().unwrap().ping();
        })
        .map_err(move |err| {
            eprintln!("Ping failed for : `{}`", addr);
            Error::from(err)
        })
        .or_else(move |_| {
            // TODO: diagnose and reconnect if necessary...
            println!("Triggering reconnection for `{}`...", addr);

            // TODO: different way to reconnect ?
            // TODO: unwrap?
            peer2.lock().unwrap().remove_socket();

            Ok(())
        })
}

// TODO: personnal PID (possibly public key)
// TODO: subfunctions
pub fn swim(local_addr: SocketAddr) -> Result<(), Error> {
    let reader = File::open("./peers.ron")?;
    let addrs: Vec<String> = ron::de::from_reader(reader).unwrap();

    // TODO: actual pid
    // create pid:
    let pid: Pid = random();
    println!("Using pid : {}", pid);

    let mut runtime = Runtime::new()?;

    let peers: PeersMapArcMut = Arc::new(Mutex::new(HashMap::new()));

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
        .filter(|addr| *addr != local_addr)
        .for_each(|addr| {
            let peer = Arc::new(Mutex::new(Peer::new(addr)));
            let peers = peers.clone();

            let peer_future =
                Interval::new(Instant::now(), Duration::from_secs(CONNECTION_INTERVAL))
                    .inspect_err(|err| eprintln!("Interval error: {:?}", err))
                    .map_err(Error::from)
                    .and_then(move |_| -> Box<Future<Item = (), Error = Error> + Send> {
                        // TODO: unwrap?
                        let state = peer.lock().unwrap().state;

                        match state {
                            PeerState::Connected => {
                                // TODO: can also use some "last time a message was sent..." to test if necessary, ...
                                // TODO: If a ping is pending, prevent sending another one ?
                                let future = ping_peer(peer.clone());
                                Box::new(future)
                            }
                            PeerState::NotConnected => {
                                let future = connect_to_peer(pid, peer.clone(), peers.clone())
                                    .map_err(move |err| {
                                        eprintln!(
                                    "Error connecting to `{}` : `{:?}`, retrying in {} seconds...",
                                    addr, err, CONNECTION_INTERVAL
                                );
                                        Error::from(err)
                                    })
                                    // Discarding errors to avoid fusing the Stream:
                                    .or_else(|_| Ok(()));
                                Box::new(future)
                            }
                            // TODO: What if peer is stuck in connecting ?
                            PeerState::Connecting(_) => Box::new(future::ok(())),
                        }
                    })
                    .for_each(|_| Ok(()))
                    .map_err(|_| ());

            runtime.spawn(peer_future);
        });

    let listener = TcpListener::bind(&local_addr)?;
    let listener_future = listener
        .incoming()
        .for_each(|socket| {
            let addr = socket.peer_addr()?;
            println!("Asked for connection : `{}`", addr);

            let framed_sock = Framed::new(socket.try_clone()?, MessageCodec::new());

            let manager = framed_sock
                .for_each(move |msg| {
                    // TODO: unwrap?
                    let socket = socket.try_clone().unwrap();
                    match msg {
                        Message::Ping => {
                            let framed_sock = Framed::new(socket, MessageCodec::new());
                            let send_future =
                                framed_sock
                                    .send(Message::Pong)
                                    .map(|_| ())
                                    .map_err(move |err| {
                                        eprintln!("{} : Could no send `Pong` : {:?}", addr, err)
                                    });

                            tokio::spawn(send_future);
                        }
                        _ => println!("{} : received a message !", addr),
                    }
                    Ok(())
                })
                .map_err(move |err| {
                    eprintln!("{} : error when receiving a message : {:?}.", addr, err)
                });

            tokio::spawn(manager);

            Ok(())
        })
        .map_err(|err| eprintln!("{:?}", err));

    runtime.spawn(listener_future);

    runtime
        .shutdown_on_idle()
        .wait()
        .map_err(|_| Error::TokioRuntimeError)
}
