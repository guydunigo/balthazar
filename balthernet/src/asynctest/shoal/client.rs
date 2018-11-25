use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::timer::Interval;

use balthmessage::Message;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// TODO: local Error
use super::Error;
use super::*;

/// Interval between connections tries in seconds
const CONNECTION_INTERVAL: u64 = 10;

fn to_connecting(peer: &mut Peer, socket: TcpStream) {
    let local_vote = peer.client_to_connecting();
    send_message_and_spawn(socket, Message::Vote(local_vote));
}

fn for_each_message_connecting(
    shoal: ShoalReadArc,
    peer_opt: PeerArcMutOpt,
    peer_addr: SocketAddr,
    socket: TcpStream,
    msg: Message,
) -> Result<(), Error> {
    let mut peer_opt = peer_opt.lock().unwrap();
    if let Some(ref peer) = *peer_opt {
        // println!("Client : {} : `peer_opt` is `Some()`.", peer_addr);

        let mut peer_locked = peer.lock().unwrap();

        /*
        println!(
            "Client : {} : `peer.state` is `{:?}`.",
            peer_addr, peer_locked.state
        );
        */
        match peer_locked.state {
            PeerState::Connecting(_) => {
                match msg {
                    Message::ConnectAck => {
                        if !peer_locked.listener_connecting {
                            peer_locked.client_connection_acked(peer.clone(), socket);
                        } else {
                            // TODO: find a way to check that listener cancelled, ... oneshot?
                            unimplemented!();
                        }
                    },
                    Message::ConnectCancel => {
                        peer_locked.client_connection_cancelled();
                        // End the message listening loop :
                        return Err(Error::ConnectionCancelled);
                    },
                    Message::Vote(_peer_vote) => {
                        if peer_locked.listener_connecting {
                            // TODO: send vote to listener
                            unimplemented!();
                        } else {
                            // TODO: what to do ?
                            unimplemented!();
                        }
                    },
                    Message::Connect(_) => {
                        // TODO: might mean that it is already connecting (listener in vote or other client)...
                        // TODO: check if same id...
                    }
                    _ => eprintln!("Client : {} : received a message but it was not `ConnectAck`, `ConnectCancel` or `Vote(vote)` : `{}`.", peer_addr, msg),
                }
            }
            PeerState::Connected(_) => {
                println!(
                    "Client : {} : `peer.state` is `Connected`, stopping connection loop.",
                    peer_addr
                );

                // TODO: keep the same receiving frame and just transfer some channel or so...
                peer_locked
                    .handle_msg(msg)
                    .expect(&format!("Client : {} : Error forwarding msg...", peer_addr)[..]);

                // End the message listening loop :
                return Err(Error::ConnectionEnded);
            }
            PeerState::NotConnected => {
                if let Message::Connect(peer_pid) = msg {
                    if peer_locked.pid() != peer_pid {
                        eprintln!("Client : {} : Received a `peer_id` that differs from the already known `peer.pid` : `peer_id=={}`, `peer.peer_id=={}`.", peer_locked.addr, peer_pid, peer_locked.pid());
                        // TODO: return error and cancel connection or create new peer ?
                    }
                }

                to_connecting(&mut peer_locked, socket);
            }
        }
    } else {
        // println!("Client : {} : `peer_opt` is `None`.", peer_addr);
        match msg {
            Message::Connect(peer_pid) => {
                let peers = shoal.lock().peers();
                let mut peers_locked = peers.lock().unwrap();

                if let Some(peer_from_peers) = peers_locked.get(&peer_pid) {
                    // println!("Client : {} : Peer is in peers.", peer_addr);
                    *peer_opt = Some(peer_from_peers.clone());

                    let mut peer = peer_from_peers.lock().unwrap();

                    println!(
                        "Client : {} : `peer.state` is `{:?}`.",
                        peer_addr, peer.state
                    );

                    match peer.state {
                        PeerState::NotConnected => {
                            if peer.pid() != peer_pid {
                                eprintln!("Client : {} : Received a `peer_id` that differs from the already known `peer.pid` : `peer_id=={}`, `peer.peer_id=={}`.", peer.addr, peer_pid, peer.pid());
                                // TODO: return error and cancel connection ?
                            }

                            to_connecting(&mut peer, socket);
                        }
                        PeerState::Connected(_) => {
                            peer.client_connection_cancel(socket);
                            // End the message listening loop :
                            return Err(Error::ConnectionCancelled);
                        }
                        PeerState::Connecting(_) => {
                            if peer.client_connecting {
                                // eprintln!("Client : {} : Peer inconsistency or double connection tasks : `peer.state` is already `Connecting(vote)`, cancelling connection...", peer_addr);

                                send_message_and_spawn(socket, Message::ConnectCancel);
                                return Err(Error::ConnectionCancelled);
                            } else if peer.listener_connecting {
                                unimplemented!("Can we arrive here ? (it would mean that listener is waiting for a vote but client was cancelled).");
                            } else {
                                panic!("Client : {} : Peer inconsistency : `peer.state` is `Connecting` but `listener_connecting` and `client_connecting` are both false.", peer_addr);
                            }
                        }
                    }
                } else {
                    // println!("Client : {} : Peer is not in peers.", peer_addr);

                    let mut peer = Peer::new(shoal.clone(), peer_pid, peer_addr);
                    to_connecting(&mut peer, socket);

                    let peer = Arc::new(Mutex::new(peer));
                    *peer_opt = Some(peer.clone());

                    shoal.lock().insert_peer(&mut *peers_locked, peer);
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
    shoal: ShoalReadArc,
    peer_opt: PeerArcMutOpt,
    peer_addr: SocketAddr,
) -> impl Future<Item = (), Error = Error> {
    let local_pid = shoal.lock().local_pid();

    TcpStream::connect(&peer_addr)
        .map_err(Error::from)
        .and_then(move |socket| {
            println!("Client : {} : starting connection...", peer_addr);
            send_message(socket, Message::Connect(local_pid))
        })
        .map_err(Error::from)
        .and_then(move |framed_sock| {
            let shoal = shoal.clone();
            let socket = framed_sock.get_ref().try_clone().unwrap();

            framed_sock
                .map_err(Error::from)
                .for_each(move |msg| {
                    for_each_message_connecting(
                        shoal.clone(),
                        peer_opt.clone(),
                        peer_addr,
                        socket.try_clone().unwrap(),
                        msg,
                    )
                })
                .map_err(move |err| {
                    match err {
                        // TODO: println anyway ?
                        Error::ConnectionCancelled | Error::ConnectionEnded => (),
                        _ => eprintln!(
                            "Client : {} : error when receiving a message : {:?}.",
                            peer_addr, err
                        ),
                    }
                    err
                })
        })
}

pub fn try_connecting_at_interval(
    shoal: ShoalReadArc,
    peer_addr: SocketAddr,
) -> impl Future<Item = (), Error = ()> {
    let peer_opt: PeerArcMutOpt = Arc::new(Mutex::new(None));
    let is_a_client_connecting = Arc::new(Mutex::new(false));

    Interval::new(Instant::now(), Duration::from_secs(CONNECTION_INTERVAL))
        .inspect_err(|err| eprintln!("Client : Interval error: {:?}", err))
        .map_err(Error::from)
        .for_each(move |_| -> Box<Future<Item = (), Error = Error> + Send> {
            let mut is_a_client_connecting_locked = is_a_client_connecting.lock().unwrap();
            if !*is_a_client_connecting_locked {
                *is_a_client_connecting_locked = true;

                let do_connect = if let Some(ref peer) = *peer_opt.lock().unwrap() {
                    if let PeerState::NotConnected = peer.lock().unwrap().state {
                        /*
                        println!(
                            "Client : {} : `peer.state` is `NotConnected`, connecting...",
                            peer_addr
                        );
                        */
                        true
                    } else {
                        /*
                        println!(
                            "Client : {} : `peer.state` is not `NotConnected`, cancelling connection. Retrying in {} seconds...",
                            peer_addr,
                            CONNECTION_INTERVAL
                        );
                        */
                        false
                    }
                } else {
                    /*
                    println!(
                        "Client : {} : `peer_opt` is `None`, connecting...",
                        peer_addr
                    );
                    */
                    true
                };

                if do_connect {
                    let is_a_client_connecting = is_a_client_connecting.clone();
                    Box::new(
                        connect_to_peer(shoal.clone(), peer_opt.clone(), peer_addr).or_else(
                            move |err| {
                                match err {
                                    Error::ConnectionEnded | Error::ConnectionCancelled => (),
                                    _ => eprintln!(
                                    "Client : {} : Error while connecting : `{:?}`. Retrying in {} seconds...",
                                    peer_addr, err, CONNECTION_INTERVAL
                                )
                                }

                                *(is_a_client_connecting.lock().unwrap()) = false;
                                Ok(())
                            }
                        )
                    )
                } else {
                    Box::new(future::ok(()))
                }
            } else {
                Box::new(future::ok(()))
            }
        })
        .map_err(|_| ())
}
