use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::timer::Interval;

use balthmessage::Message;

use std::boxed::Box;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// TODO: local Error
use super::peer::*;
use super::Error;

/// Interval between connections tries in seconds
const CONNECTION_INTERVAL: u64 = 10;

fn vote(peer: &mut Peer, socket: TcpStream) {
    let local_vote = peer.client_to_connecting();
    send_message_and_spawn(socket, Message::Vote(local_vote));
}

fn for_each_message_connecting(
    _local_pid: Pid,
    peers: PeersMapArcMut,
    peer_opt: PeerArcMutOpt,
    peer_addr: SocketAddr,
    socket: TcpStream,
    msg: &Message,
) -> Result<(), Error> {
    // TODO: lock for the whole block?
    let mut peer_opt = peer_opt.lock().unwrap();
    if let Some(ref peer) = *peer_opt {
        println!("Client : {} : `peer_opt` is `Some()`.", peer_addr);

        // TODO: keep lock ?
        let state = peer.lock().unwrap().state;
        println!("Client : {} : `peer.state` is `{:?}`.", peer_addr, state);

        match state {
            PeerState::Connecting(_) => {
                match msg {
                    Message::ConnectAck => {
                        let mut peer = peer.lock().unwrap();

                        if !peer.listener_connecting {
                            peer.client_connection_acked(socket);
                        } else {
                            unimplemented!();
                        }
                    },
                    Message::ConnectCancel => {
                        // TODO: close socket
                        unimplemented!();
                    },
                    Message::Vote(peer_vote) => {
                        let peer = peer.lock().unwrap();

                        if peer.listener_connecting {
                            // TODO: send vote to listener
                            unimplemented!();
                        } else {
                            // TODO: what to do ?
                            unimplemented!();
                        }
                    },
                    _ => eprintln!("Client : {} : received a message but it was not `ConnectAck`, `ConnectCancel` or `Vote(vote)`.", peer_addr),
                }
            }
            PeerState::Connected => {
                println!(
                    "Client : {} : `peer.state` is `Connected`, stopping connecting loop.",
                    peer_addr
                );
                // End the message listening loop :
                return Err(Error::ConnectionEnded);
            }
            _ => panic!(
                "Client : {} : `peer.state` shouldn't be `{:?}` when `peer_opt` is `Some(peer)`.",
                peer_addr, state
            ),
        }
    } else {
        println!("Client : {} : `peer_opt` is `None`.", peer_addr);
        match msg {
            Message::ConnectReceived(peer_pid) => {
                // TODO: lock for the whole block
                let peer_from_peers = {
                    let peers = peers.lock().unwrap();
                    match peers.get(&peer_pid) {
                        Some(peer) => Some(peer.clone()),
                        None => None,
                    }
                };

                if let Some(peer_from_peers) = peer_from_peers {
                    println!("Client : {} : Peer is in peers.", peer_addr);
                    *peer_opt = Some(peer_from_peers.clone());

                    let mut peer = peer_from_peers.lock().unwrap();

                    println!(
                        "Client : {} : `peer.state` is `{:?}`.",
                        peer_addr, peer.state
                    );
                    match peer.state {
                        PeerState::NotConnected => {
                            // TODO: check if same id ?
                            peer.set_pid(*peer_pid);
                            vote(&mut peer, socket);
                        }
                        PeerState::Connected => {
                            peer.client_connection_cancel();
                            // End the message listening loop :
                            return Err(Error::ConnectionCancelled);
                        }
                        PeerState::Connecting(_) => {
                            if peer.client_connecting {
                                panic!("Client : {} : Peer inconsistency or double connection tasks : `peer.state` is already `Connecting(vote)`.", peer_addr);
                            // TODO: do something ? like return close loop, ...
                            } else if peer.listener_connecting {
                                unimplemented!("Can we arrive here ? (it would mean that listener is waiting for a vote but client was cancelled).");
                            } else {
                                panic!("Client : {} : Peer inconsistency : `peer.state` is `Connecting` but `listener_connecting` and `client_connecting` are both false.", peer_addr);
                            }
                        }
                    }
                } else {
                    let mut peers = peers.lock().unwrap();
                    println!("Client : {} : Peer is not in peers.", peer_addr);

                    let mut peer = Peer::new(*peer_pid, peer_addr);
                    vote(&mut peer, socket);

                    let peer = Arc::new(Mutex::new(peer));
                    *peer_opt = Some(peer.clone());
                    peers.insert(*peer_pid, peer);
                }
            }
            _ => eprintln!(
                "Client : {} : received a message but it was not `ConnectReceived(pid)`.",
                peer_addr
            ),
        }
    }
    Ok(())
}

fn connect_to_peer(
    local_pid: Pid,
    peer_addr: SocketAddr,
    peers: PeersMapArcMut,
) -> impl Future<Item = (), Error = Error> {
    TcpStream::connect(&peer_addr)
        .map_err(Error::from)
        .and_then(move |socket| send_message(socket, Message::Connect(local_pid)))
        .and_then(move |framed_sock| {
            let socket = framed_sock.get_ref().try_clone().unwrap();

            println!("Client : starting connection for `{}`.", peer_addr);

            let peer = Arc::new(Mutex::new(None));

            let manager = framed_sock
                .map_err(Error::from)
                .for_each(move |msg| {
                    for_each_message_connecting(
                        local_pid,
                        peers.clone(),
                        peer.clone(),
                        peer_addr,
                        socket.try_clone().unwrap(),
                        &msg,
                    )
                })
                .map_err(move |err| match err {
                    // TODO: println anyway ?
                    Error::ConnectionCancelled | Error::ConnectionEnded => (),
                    _ => eprintln!(
                        "Client : {} : error when receiving a message : {:?}.",
                        peer_addr, err
                    ),
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
        // TODO: check if connected ?
        let socket = if let Some(socket) = &peer.socket {
            // TODO: unwrap?
            socket.try_clone().unwrap()
        } else {
            panic!("Client : Peer has no Socket, can't Ping !");
        };

        (peer.addr, socket)
    };

    send_message(socket, Message::Ping)
        // TODO: is this map useful ?
        .map(move |_| {
            // TODO: send_message in ping ?
            // TODO: unwrap?
            peer.lock().unwrap().ping();
        })
        .map_err(move |err| {
            eprintln!("Client : Ping failed for : `{}`", addr);
            Error::from(err)
        })
        .or_else(move |_| {
            // TODO: diagnose and reconnect if necessary...
            println!("Client : Triggering reconnection for `{}`...", addr);

            // TODO: different way to reconnect ?
            // TODO: unwrap?
            peer2.lock().unwrap().disconnect();

            Ok(())
        })
}

pub fn connect(
    local_pid: Pid,
    peer_addr: SocketAddr,
    peers: PeersMapArcMut,
) -> impl Future<Item = (), Error = ()> {
    connect_to_peer(local_pid, peer_addr, peers).map_err(|_| ())
    /*
    let peer = Arc::new(Mutex::new(Peer::new(addr)));
    
    Interval::new(Instant::now(), Duration::from_secs(CONNECTION_INTERVAL))
        .inspect_err(|err| eprintln!("Client : Interval error: {:?}", err))
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
                                "Client : Error connecting to `{}` : `{:?}`, retrying in {} seconds...",
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
        .map_err(|_| ())
        */
}
