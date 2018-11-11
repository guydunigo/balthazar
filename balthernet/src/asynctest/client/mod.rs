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
    local_pid: Pid,
    peers: PeersMapArcMut,
    peer_opt: PeerArcMutOpt,
    peer_addr: SocketAddr,
    socket: TcpStream,
    msg: &Message,
) -> Result<(), Error> {
    let mut peer_opt = peer_opt.lock().unwrap();
    if let Some(ref peer) = *peer_opt {
        // println!("Client : {} : `peer_opt` is `Some()`.", peer_addr);

        let mut peer_locked = peer.lock().unwrap();

        // println!("Client : {} : `peer.state` is `{:?}`.", peer_addr, peer_locked.state);
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
                    _ => eprintln!("Client : {} : received a message but it was not `ConnectAck`, `ConnectCancel` or `Vote(vote)`.", peer_addr),
                }
            }
            PeerState::Connected(_) => {
                println!(
                    "Client : {} : `peer.state` is `Connected`, stopping connection loop.",
                    peer_addr
                );
                // End the message listening loop :
                return Err(Error::ConnectionEnded);
            }
            PeerState::NotConnected => {
                vote(&mut peer_locked, socket);
            } /*
              _ => panic!(
                  "Client : {} : `peer.state` shouldn't be `{:?}` when `peer_opt` is `Some(peer)`.",
                  peer_addr, state
              ),
              */
        }
    } else {
        // println!("Client : {} : `peer_opt` is `None`.", peer_addr);
        match msg {
            Message::ConnectReceived(peer_pid) => {
                let mut peers_locked = peers.lock().unwrap();

                if let Some(peer_from_peers) = peers_locked.get(&peer_pid) {
                    // println!("Client : {} : Peer is in peers.", peer_addr);
                    *peer_opt = Some(peer_from_peers.clone());

                    let mut peer = peer_from_peers.lock().unwrap();

                    /*
                    println!(
                        "Client : {} : `peer.state` is `{:?}`.",
                        peer_addr, peer.state
                    );
                    */
                    match peer.state {
                        PeerState::NotConnected => {
                            if peer.peer_pid() != *peer_pid {
                                eprintln!("Client : {} : Received a `peer_id` that differs from the already known `peer.peer_pid` : `peer_id=={}`, `peer.peer_id=={}`.", peer.addr, peer_pid, peer.peer_pid());
                                // TODO: return error and cancel connection ?
                            }

                            vote(&mut peer, socket);
                        }
                        PeerState::Connected(_) => {
                            peer.client_connection_cancel(socket);
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
                    // println!("Client : {} : Peer is not in peers.", peer_addr);

                    let mut peer = Peer::new(local_pid, *peer_pid, peer_addr, peers.clone());
                    vote(&mut peer, socket);

                    let peer = Arc::new(Mutex::new(peer));
                    *peer_opt = Some(peer.clone());
                    peers_locked.insert(*peer_pid, peer);
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
    peer_opt: PeerArcMutOpt,
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

            let manager = framed_sock
                .map_err(Error::from)
                .for_each(move |msg| {
                    for_each_message_connecting(
                        local_pid,
                        peers.clone(),
                        peer_opt.clone(),
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

pub fn try_connecting_at_interval(
    local_pid: Pid,
    peer_addr: SocketAddr,
    peers: PeersMapArcMut,
) -> impl Future<Item = (), Error = ()> {
    let peer_opt: PeerArcMutOpt = Arc::new(Mutex::new(None));

    Interval::new(Instant::now(), Duration::from_secs(CONNECTION_INTERVAL))
        .inspect_err(|err| eprintln!("Client : Interval error: {:?}", err))
        .map_err(Error::from)
        .and_then(move |_| -> Box<Future<Item = (), Error = Error> + Send> {
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
                Box::new(
                    connect_to_peer(peer_opt.clone(), local_pid, peer_addr, peers.clone()).or_else(
                        move |err| {
                            eprintln!(
                                "Client : {} : Error while connecting : `{:?}`. Retrying in {} seconds...",
                                peer_addr, err, CONNECTION_INTERVAL
                            );
                            Ok(())
                        }
                    )
                )
            } else {
                Box::new(future::ok(()))
            }
        })
        .for_each(|_| Ok(()))
        .map_err(|_| ())
}
