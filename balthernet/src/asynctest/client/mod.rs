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
use super::{Error, MessageCodec};
use balthmessage as message;

/// Interval between connections tries in seconds
const CONNECTION_INTERVAL: u64 = 10;

pub fn for_each_message(
    addr: SocketAddr,
    peer: PeerArcMut,
    peers: PeersMapArcMut,
    socket: TcpStream,
    msg: &Message,
) -> Result<(), message::Error> {
    let state = peer.lock().unwrap().state;

    match state {
        PeerState::Connecting(_) => match msg { // TODO: lock for the whole block ?
            Message::ConnectReceived(pid) => {
                peer.lock().unwrap().set_pid(*pid);

                // TODO: double lock...
                // TODO: can this lock kill the mutex ?
                if let Some(_) = peers.lock().unwrap().get(&pid) {
                    // TODO: check that there can be a connectAck, cancel listener
                    // procedure if necessary...
                    unimplemented!("There is already a peer in peers");
                } else {
                    peers.lock().unwrap().insert(*pid, peer.clone());
                }
            },
            Message::ConnectAck => {
                // TODO: unwrap?
                let mut peer = peer.lock().unwrap();
                // TODO: cloning twice the socket ?
                peer.client_connection_acked(socket.try_clone().unwrap());
            },
            Message::ConnectCancel => {
                peer.lock().unwrap().client_connection_cancelled();
                // TODO: kill this loop...
                unimplemented!();
            },
            _ => unimplemented!(),
        },
        PeerState::Connected => for_each_message_connected(addr, peer, msg)?,
        PeerState::NotConnected => panic!("Inconsistent Peer object : a message was received, but `peer.state` is `PeerState::NotConnected`."),
    }
    Ok(())
}

fn connect_to_peer(
    pid: Pid,
    peer: PeerArcMut,
    peers: PeersMapArcMut,
) -> impl Future<Item = (), Error = Error> {
    // TODO: unwrap?
    let addr = peer.lock().unwrap().addr;
    let peer2 = peer.clone();

    TcpStream::connect(&addr)
        .map_err(Error::from)
        .and_then(move |socket| {
            // TODO: unwrap?
            let local_vote = peer2.lock().unwrap().client_to_connecting();
            send_message(socket, Message::Connect(pid, local_vote))
        })
        .and_then(move |framed_sock| {
            let socket = framed_sock.get_ref().try_clone().unwrap();

            let manager = framed_sock
                .for_each(move |msg| {
                    for_each_message(
                        addr,
                        peer.clone(),
                        peers.clone(),
                        socket.try_clone().unwrap(),
                        &msg,
                    )
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

    send_message(socket, Message::Ping)
        // TODO: is this map useful ?
        .map(move |_| {
            // TODO: send_message in ping ?
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
            peer2.lock().unwrap().disconnect();

            Ok(())
        })
}

pub fn connect(
    pid: Pid,
    addr: SocketAddr,
    peers: PeersMapArcMut,
) -> impl Future<Item = (), Error = ()> {
    let peer = Arc::new(Mutex::new(Peer::new(addr)));

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
        .map_err(|_| ())
}
