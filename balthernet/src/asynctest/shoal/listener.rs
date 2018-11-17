use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use super::*;
// TODO: local Error
use super::Error;
use balthmessage::Message;

/// **Important** : peer must be a reference to self.
fn handle_vote(
    socket: TcpStream,
    peer_locked: &mut Peer,
    peer: PeerArcMut,
    local_vote: ConnVote,
    peer_vote: ConnVote,
) -> Result<(), Error> {
    if local_vote < peer_vote {
        println!("Listener : Vote : peer won, cancelling connection...");
        peer_locked.listener_connection_cancel(socket);
        return Err(Error::ConnectionCancelled);
    } else if local_vote > peer_vote {
        println!("Listener : Vote : peer lost, validating connection...");
        peer_locked.listener_connection_ack(peer, socket);
    } else {
        println!("Listener : Vote : Equality, sending new vote...");
        let new_local_vote = peer_locked.listener_to_connecting();
        peer_locked.send_and_spawn(Message::Vote(new_local_vote));
    }

    Ok(())
}

fn for_each_message_connecting(
    shoal: ShoalReadArc,
    peer_opt: PeerArcMutOpt,
    peer_addr: SocketAddr,
    socket: TcpStream,
    msg: &Message,
) -> Result<(), Error> {
    let mut peer_opt = peer_opt.lock().unwrap();
    if let Some(ref peer) = *peer_opt {
        // println!("Listener : `peer_opt` is `Some()`.");

        let mut peer_locked = peer.lock().unwrap();

        // println!("Listener : `peer.state` is `{:?}`.", peer_locked.state);
        match peer_locked.state {
            PeerState::Connecting(local_vote) => {
                if peer_locked.client_connecting {
                    match msg {
                        Message::Vote(peer_vote) => handle_vote(
                            socket,
                            &mut *peer_locked,
                            peer.clone(),
                            local_vote,
                            *peer_vote,
                        )?,
                        _ => {
                            eprintln!("Listener : received a message but it was not `Vote(vote)`.")
                        }
                    }
                } else {
                    peer_locked.listener_connection_ack(peer.clone(), socket);
                    // End the message listening loop :
                    return Err(Error::ConnectionEnded);
                }
            }
            PeerState::Connected(_) => {
                println!("Listener : `peer.state` is `Connected`, stopping connection loop.");
                // End the message listening loop :
                return Err(Error::ConnectionEnded);
            }
            _ => eprintln!(
                "Listener : `peer.state` shouldn't be `{:?}` when `peer_opt` is `Some(peer)`.",
                peer_locked.state
            ),
        }
    } else {
        // println!("Listener : `peer_opt` is `None`.");
        match msg {
            Message::Connect(peer_pid) => {
                let peers = shoal.lock().peers();
                let peers_locked = peers.lock().unwrap();

                if let Some(peer_from_peers) = peers_locked.get(&peer_pid) {
                    // println!("Listener : {} : Peer is in peers.", peer_addr);
                    *peer_opt = Some(peer_from_peers.clone());

                    let mut peer = peer_from_peers.lock().unwrap();

                    // println!("Listener : `peer.state` is `{:?}`.", peer.state);
                    match peer.state {
                        PeerState::NotConnected => {
                            if peer.pid() != *peer_pid {
                                eprintln!("Client : {} : Received a `peer_id` that differs from the already known `peer.pid` : `peer_id=={}`, `peer.peer_id=={}`.", peer.addr, peer_pid, peer.pid());
                                // TODO: return error and cancel connection ?
                            }

                            peer.listener_connection_ack(peer_from_peers.clone(), socket);
                            // End the message listening loop :
                            return Err(Error::ConnectionEnded);
                        }
                        PeerState::Connected(_) => {
                            // eprintln!("Listener : Someone tried to connect with pid `{}` but it is already connected (`state` is `Connected`). Cancelling...", peer_pid);
                            cancel_connection(socket);
                            // End the message listening loop :
                            return Err(Error::ConnectionCancelled);
                        }
                        PeerState::Connecting(_local_vote) => {
                            if peer.listener_connecting {
                                // eprintln!("Listener : Someone tried to connect with pid `{}` but it is in connection with a listener (`state` is `Connected` and `listener_connecting` is `true`). Cancelling...", peer_pid);
                                cancel_connection(socket);
                                return Err(Error::ConnectionCancelled);
                            } else if !peer.client_connecting {
                                panic!("Listener : Peer inconsistency : `state` is `Connecting` but `listener_connecting` and `client_connecting` are both false.");
                            }
                        }
                    }
                } else {
                    // println!("Client : {} : Peer is not in peers.", peer_addr);

                    let peer = Peer::new(shoal.clone(), *peer_pid, peer_addr);
                    let peer_arc_mut = Arc::new(Mutex::new(peer));

                    {
                        let mut peer = peer_arc_mut.lock().unwrap();
                        peer.listener_connection_ack(peer_arc_mut.clone(), socket);
                    }

                    *peer_opt = Some(peer_arc_mut.clone());
                    shoal.lock().insert_peer(peer_arc_mut);
                }
            }
            _ => eprintln!("Listener : received a message but it was not `Connect(pid,vote)`."),
        }
    }
    Ok(())
}

pub fn bind(local_addr: &SocketAddr) -> Result<TcpListener, io::Error> {
    TcpListener::bind(local_addr)
}

pub fn listen(shoal: ShoalReadArc, listener: TcpListener) -> impl Future<Item = (), Error = ()> {
    listener
        .incoming()
        .for_each(move |socket| {
            let local_pid = shoal.lock().local_pid;

            let shoal = shoal.clone();
            let peer_addr = socket.peer_addr()?;
            println!("Listener : Asked for connection : `{}`", peer_addr);

            let peer = Arc::new(Mutex::new(None));

            let send_future =
                send_message(socket.try_clone()?, Message::ConnectReceived(local_pid))
                    .and_then(move |framed_sock| {
                        let manager = framed_sock
                            .map_err(Error::from)
                            .for_each(move |msg| {
                                for_each_message_connecting(
                                    shoal.clone(),
                                    peer.clone(),
                                    peer_addr,
                                    // TODO: unwrap?
                                    socket.try_clone().unwrap(),
                                    &msg,
                                )
                            })
                            .map_err(move |err| match err {
                                // TODO: println anyway ?
                                Error::ConnectionCancelled | Error::ConnectionEnded => (),
                                _ => eprintln!(
                                    "Listener : error when receiving a message : {:?}.",
                                    err
                                ),
                            });

                        tokio::spawn(manager);

                        Ok(())
                    })
                    .map_err(|_| ());

            tokio::spawn(send_future);

            Ok(())
        })
        .map_err(|err| eprintln!("Listener : {:?}", err))
}
