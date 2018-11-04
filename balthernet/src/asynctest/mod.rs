use tokio::codec::Framed;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::timer::{Delay, Interval};

use std::time::{Duration, Instant};

use balthmessage::Message;

use std::fs::File;
use std::net::SocketAddr;

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
// TODO: no lifetime ?
pub fn connect_peer<'a>(addr: &'a SocketAddr) -> impl Future<Item = (), Error = ()> + 'a {
    TcpStream::connect(addr)
        .and_then(move |socket| {
            println!("Connected to : `{}`", addr);

            let framed_sock = Framed::new(socket, MessageCodec::new());

            /*
            // TODO: process to check in case of collision and to assign pid?
            // TODO: proper number (random ?)
            let proposed_pid = 0;
            let future = framed_sock.send(Message::Connect(proposed_pid)).and_then(|framed| {
                framed_sock.take(1).for_each(|msg| {
                    if let Message::Connected(received_pid) = msg {
            
                    } else {
                        Err(Error::FailedHandshake)
                    }
                })
            });
            println!("{} : Handshake successful.", pid);
            */
            let pid = 0;

            let manager = framed_sock.take(1).for_each(|msg| {
                println!("{} : received a message !", pid);
                Ok(())
            });

            task::spawn(manager);

            // TODO: maybe send `Peer { socket, pid, addr }` to another thread...
            Ok(())
        })
        .map_err(|err| eprintln!("Could not connect : {:?}", err))
}

// TODO: subfunctions
pub fn swim(local_addr: SocketAddr) -> Result<(), Error> {
    let reader = File::open("./peers.ron")?;
    let addrs: Vec<String> = ron::de::from_reader(reader).unwrap();

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
            let peer_future =
                Interval::new(Instant::now(), Duration::from_secs(CONNECTION_INTERVAL))
                    // TODO: Or don't stop retrying, do some connection check ?
                    .map_err(|err| Error::from(err))
                    .and_then(move |_| TcpStream::connect(&addr).map_err(|err| Error::from(err)))
                    .inspect_err(move |err| {
                        eprintln!(
                            "Error connecting to `{}` : `{:?}`, retrying in {} seconds...",
                            addr, err, CONNECTION_INTERVAL
                        );
                    })
                    .take(1)
                    .and_then(move |socket| {
                        println!("Connected to : `{}`", addr);

                        let send_msg =
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
                    .into_future()
                    .map(|(elm, _)| elm.unwrap())
                    .map_err(|_| ());

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
                .for_each(move |msg| {
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
