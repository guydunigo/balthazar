use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

// TODO: local Error
use super::peer::*;
use super::Error;
use balthmessage::Message;

fn handle_vote(socket: TcpStream, peer: &mut Peer, local_vote: ConnVote, peer_vote: ConnVote) {
    if local_vote < peer_vote {
        println!("Listener : Vote : peer won, cancelling connection...");
        peer.listener_connection_cancel();
    // TODO: stop the loop...
    } else if local_vote > peer_vote {
        println!("Listener : Vote : peer lost, validating connection...");
        peer.listener_connection_ack(socket);
    } else {
        println!("Listener : Vote : Equality, sending new vote...");
        let new_local_vote = peer.listener_to_connecting();
        peer.send_and_spawn(Message::Vote(new_local_vote));
    }
}

// TODO: rename everything everywhere with {peer,local}_{pid,vote,...}
// TODO: properly define the algorithm here
fn for_each_message_connecting(
    _local_pid: Pid,
    peers: PeersMapArcMut,
    peer_opt: PeerArcMutOpt,
    peer_addr: SocketAddr,
    socket: TcpStream,
    msg: &Message,
) -> Result<(), Error> {
    // TODO: lock for the whole block?
    if let Some(peer) = peer_opt.lock().unwrap().clone() {
        println!("Listener : `peer_opt` is `Some()`.");

        // TODO: keep lock ?
        let state = peer.lock().unwrap().state;
        println!("Listener : `peer.state` is `{:?}`.", state);

        match state {
            PeerState::Connecting(local_vote) => {
                let mut peer = peer.lock().unwrap();

                if peer.client_connecting {
                    match msg {
                        Message::Vote(peer_vote) => {
                            handle_vote(socket, &mut *peer, local_vote, *peer_vote)
                        }
                        _ => {
                            eprintln!("Listener : received a message but it was not `Vote(vote)`.")
                        }
                    }
                } else {
                    peer.listener_connection_ack(socket);
                }
            }
            _ => panic!(
                "Listener : `peer.state` shouldn't be `{:?}` when `peer_opt` is `Some(peer)`.",
                state
            ),
        }
    } else {
        println!("Listener : `peer_opt` is `None`.");
        match msg {
            Message::Connect(peer_pid) => {
                // TODO: lock for the whole block
                let peer_from_peers = {
                    let peers = peers.lock().unwrap();
                    match peers.get(&peer_pid) {
                        Some(peer) => Some(peer.clone()),
                        None => None,
                    }
                };

                if let Some(peer_from_peers) = peer_from_peers {
                    *(peer_opt.lock().unwrap()) = Some(peer_from_peers.clone());

                    let mut peer = peer_from_peers.lock().unwrap();

                    println!("Listener : `peer.state` is `{:?}`.", peer.state);
                    match peer.state {
                        PeerState::NotConnected => {
                            // TODO: check if same id ?
                            peer.set_pid(*peer_pid);
                            peer.listener_connection_ack(socket);
                        }
                        // TODO: kill this loop... (is closing the socket sufficient for stopping the loop ?)
                        PeerState::Connected => {
                            eprintln!("Listener : Someone tried to connect with pid `{}` but it is already connected (`state` is `Connected`). Cancelling...", peer_pid);
                            cancel_connection(socket);
                        }
                        PeerState::Connecting(local_vote) => {
                            // TODO: kill this loop... (is closing the socket sufficient for stopping the loop ?)
                            if peer.listener_connecting {
                                eprintln!("Listener : Someone tried to connect with pid `{}` but it is in connection with a listener (`state` is `Connected` and `listener_connecting` is `true`). Cancelling...", peer_pid);
                            } else if !peer.client_connecting {
                                panic!("Listener : Peer inconsistency : `state` is `Connecting` but `listener_connecting` and `client_connecting` are both false.");
                            }
                        }
                    }
                } else {
                    let mut peers = peers.lock().unwrap();

                    let mut peer = Peer::new(*peer_pid, peer_addr);
                    peer.listener_connection_ack(socket);

                    let peer = Arc::new(Mutex::new(peer));
                    *(peer_opt.lock().unwrap()) = Some(peer.clone());
                    peers.insert(*peer_pid, peer);
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

pub fn listen(
    local_pid: Pid,
    peers: PeersMapArcMut,
    listener: TcpListener,
) -> impl Future<Item = (), Error = ()> {
    listener
        .incoming()
        .for_each(move |socket| {
            let peers = peers.clone();
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
                                    local_pid,
                                    peers.clone(),
                                    peer.clone(),
                                    peer_addr,
                                    // TODO: unwrap?
                                    socket.try_clone().unwrap(),
                                    &msg,
                                )
                            })
                            .map_err(move |err| {
                                eprintln!("Listener : error when receiving a message : {:?}.", err)
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
