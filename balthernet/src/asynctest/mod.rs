use tokio::codec::Framed;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::timer::Interval;

use balthmessage::Message;

use std::boxed::Box;
use std::fs::File;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;

pub mod message_codec;
type MessageCodec = message_codec::MessageCodec;

/// Interval between connections tries in seconds
const CONNECTION_INTERVAL: u64 = 10;

// TODO: beware of deadlocking a peer ?
// TODO: async lock?
// type PeersMap = Arc<Mutex<HashMap<SocketAddr, TcpStream>>>;
type PeerArcMut = Arc<Mutex<Peer>>;

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

#[derive(Debug)]
pub struct Peer {
    pub pid: usize,
    pub addr: SocketAddr,
    pub socket: Option<TcpStream>,
    pub ping_status: PingStatus,
}

impl Peer {
    pub fn new(addr: SocketAddr) -> Self {
        Peer {
            // TODO: do something with pid...
            pid: 0,
            addr,
            socket: None,
            ping_status: PingStatus::new(),
        }
    }

    pub fn remove_socket(&mut self) {
        self.socket = None;
        self.ping_status = PingStatus::NoPingYet;
    }

    pub fn is_already_connected(&self) -> bool {
        self.socket.is_some()
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
}

fn connect_to_peer(peer: PeerArcMut) -> impl Future<Item = (), Error = Error> {
    // TODO: unwrap?
    let addr = peer.lock().unwrap().addr;
    let peer2 = peer.clone();

    TcpStream::connect(&addr)
        .inspect(move |socket| {
            // TODO: unwrap?
            peer.lock().unwrap().socket = Some(socket.try_clone().unwrap());
            println!("Connected to : `{}`", addr);
        })
    // TODO: Move to separate function
    .and_then(move |socket| {
        let framed_sock = Framed::new(socket, MessageCodec::new());

        let manager = framed_sock
            .for_each(move |msg| {
                match msg {
                    Message::Ping => {
                        // TODO: unwrap?
                        let (addr, socket) = {
                            let peer = peer2.lock().unwrap();
                            let socket = if let Some(socket) = &peer.socket {
                                // TODO: unwrap?
                                socket.try_clone().unwrap()
                            } else {
                                panic!("Peer object inconsistent : a message was received, but `peer.socket` is `None`.");
                            };

                            (peer.addr, socket)
                        };

                        let framed_sock = Framed::new(socket, MessageCodec::new());
                        let send_future = framed_sock.send(Message::Pong).map(|_| ()).map_err(move |err| eprintln!("{} : Could no send `Pong` : {:?}", addr, err));

                        tokio::spawn(send_future);
                    },
                    Message::Pong => {
                        // TODO: unwrap?
                        peer2.lock().unwrap().pong();
                        println!("{} : received Pong ! It is alive !!!", addr);
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
    .map_err(Error::from)
}

// TODO: is it needed to use a `Pong`, or does the TCP socket returns an error if broken connection?
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

    let mut runtime = Runtime::new()?;

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

            let peer_future =
                Interval::new(Instant::now(), Duration::from_secs(CONNECTION_INTERVAL))
                    .map_err(Error::from)
                    .and_then(move |_| -> Box<Future<Item = (), Error = Error> + Send> {
                        // TODO: unwrap?
                        let is_already_connected = peer.lock().unwrap().is_already_connected();

                        if is_already_connected {
                            // TODO: can also use some "last time a message was sent..." to test if necessary, ...
                            // TODO: If a ping is pending, prevent sending another one ?
                            let future = ping_peer(peer.clone());
                            Box::new(future)
                        } else {
                            let future = connect_to_peer(peer.clone());
                            Box::new(future)
                        }
                    })
                    .inspect_err(move |err| {
                        eprintln!(
                            "Error connecting to `{}` : `{:?}`, retrying in {} seconds...",
                            addr, err, CONNECTION_INTERVAL
                        );
                    })
                    .for_each(|_| Ok(()))
                    .map_err(|err| eprintln!("Interval error: {:?}", err));

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
