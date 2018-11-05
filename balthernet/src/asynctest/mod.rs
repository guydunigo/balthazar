use tokio::codec::Framed;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::timer::Interval;

use std::time::{Duration, Instant};

use balthmessage::Message;

use std::collections::HashMap;
use std::fs::File;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use super::*;

pub mod message_codec;
type MessageCodec = message_codec::MessageCodec;

/// Interval between connections tries in seconds
const CONNECTION_INTERVAL: u64 = 10;

#[derive(Debug)]
pub struct Peer {
    pub pid: usize,
    pub addr: SocketAddr, // TODO: Usefull as it can be aquired from socket ?
    pub socket: TcpStream,
}

// TODO: personnal PID (possibly public key)
// TODO: subfunctions
pub fn swim(local_addr: SocketAddr) -> Result<(), Error> {
    let reader = File::open("./peers.ron")?;
    let addrs: Vec<String> = ron::de::from_reader(reader).unwrap();

    // TODO: beware of deadlocking a peer ?
    let peers: HashMap<SocketAddr, TcpStream> = HashMap::new();
    let peers = Arc::new(Mutex::new(peers));

    let mut runtime = Runtime::new()?;

    addrs
        .iter()
        .map(|addr| parse_socket_addr(addr))
        .filter_map(|addr| match addr {
            Ok(addr) => Some(addr),
            Err(err) => {
                eprintln!("{:?}", err);
                None
            }
        })
        .filter(|addr| *addr != local_addr)
        .for_each(|addr| {
            let peers = peers.clone();

            let peer_future =
                Interval::new(Instant::now(), Duration::from_secs(CONNECTION_INTERVAL))
                    // TODO: Or don't stop retrying, do some connection check ?
                    .map_err(|err| Error::from(err))
                    .and_then(move |_| {
                        let is_already_connected = {
                            // TODO: unwrap?
                            match peers.lock().unwrap().get(&addr) {
                                None => None,
                                Some(socket) => Some(socket.try_clone().unwrap()),
                            }
                        };

                        if let Some(_) = is_already_connected {
                            // TODO: For health check and reconnection trial (potential collision with normal messages) ?
                            // TODO: can also use some "last time a message was sent..." to test if necessary, ...
                            unimplemented!();
                        } else {
                            let addr_clone = addr.clone();
                            let peers = peers.clone();

                            TcpStream::connect(&addr)
                                .inspect(move |socket| {
                                    // TODO: unwrap? and take care of option ?
                                    peers
                                        .lock()
                                        .unwrap()
                                        .insert(addr_clone, socket.try_clone().unwrap());
                                })
                                .map_err(|err| Error::from(err))
                        }
                    })
                    .inspect_err(move |err| {
                        eprintln!(
                            "Error connecting to `{}` : `{:?}`, retrying in {} seconds...",
                            addr, err, CONNECTION_INTERVAL
                        );
                    })
                    .and_then(move |socket| {
                        println!("Connected to : `{}`", addr);

                        let send_msg =
                            // TODO: unwrap?
                            Framed::new(socket.try_clone().unwrap(), MessageCodec::new())
                                .send(Message::Hello("salut!".to_string()))
                                .map(|_| println!("sent"))
                                .map_err(|err| eprintln!("{:?}", err));

                        let framed_sock = Framed::new(socket, MessageCodec::new());

                        let pid = 0;

                        let manager = framed_sock
                            .for_each(move |_| {
                                println!("{} : received a message !", pid);
                                Ok(())
                            })
                            .map_err(move |err| {
                                eprintln!("{} : error when receiving a message : {:?}.", pid, err)
                            });

                        tokio::spawn(manager);
                        tokio::spawn(send_msg);

                        // TODO: maybe send `Peer { socket, pid, addr }` to another thread...
                        Ok(())
                    })
                    // TODO: Well, ... this is dirty :( ... (or, is it ?)
                    // TODO: It is not usable in the end, as it stops the intervals after one
                    // trial...
                    .for_each(|_| Ok(()))
                    .map_err(|err| eprintln!("Interval error: {:?}", err));

            runtime.spawn(peer_future);
        });

    let listener = TcpListener::bind(&local_addr)?;
    let listener_future = listener
        .incoming()
        .for_each(|socket| {
            println!("Asked for connection : `{}`", socket.peer_addr()?);

            let framed_sock = Framed::new(socket.try_clone()?, MessageCodec::new());

            let pid = 0;

            let manager = framed_sock
                .for_each(move |_| {
                    println!("{} : received a message !", pid);

                    let framed_sock = Framed::new(socket.try_clone()?, MessageCodec::new());

                    // TODO: Is it good to wait ? (Can it block the task or is it automigically asynced ?)
                    framed_sock
                        .send(Message::Hello("salut!".to_string()))
                        .wait()?;
                    Ok(())
                })
                .map_err(move |err| {
                    eprintln!("{} : error when receiving a message : {:?}.", pid, err)
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
