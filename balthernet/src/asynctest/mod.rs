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

/// Interval between connections tries in seconds
const CONNECTION_INTERVAL: u64 = 10;

type PeersMap = Arc<Mutex<HashMap<SocketAddr, TcpStream>>>;

#[derive(Debug)]
pub struct Peer {
    pub pid: usize,
    pub addr: SocketAddr, // TODO: Usefull as it can be aquired from socket ?
    pub socket: TcpStream,
}

fn connect_to_peer(peers: PeersMap, addr: SocketAddr) -> impl Future<Item = (), Error = Error> {
    TcpStream::connect(&addr)
        .inspect(move |socket| {
            // TODO: unwrap? and take care of option ?
            // TODO: async lock?
            peers
                .lock()
                .unwrap()
                .insert(addr, socket.try_clone().unwrap());
        })
        // TODO: Move to separate function
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

            tokio::spawn(send_msg);
            tokio::spawn(manager);

            // TODO: maybe send `Peer { socket, pid, addr }` to another thread...
            Ok(())
        })
        .map_err(|err| Error::from(err))
}

// TODO: is it needed to use a `Pong`, or does the TCP socket returns an error if broken connection?
//       also, waiting for the `Pong` at several places migth cause some messages being lost?
fn ping_peer(
    peers: PeersMap,
    socket: TcpStream,
    addr: SocketAddr,
) -> impl Future<Item = (), Error = Error> {
    let addr2 = addr.clone();
    Framed::new(socket, MessageCodec::new())
        .send(Message::Ping)
        // TODO: is this map useful ?
        .map(|_| ())
        .map_err(move |err| {
            eprintln!("Ping failed for : `{}`", addr);
            Error::from(err)
        })
        .or_else(move |_| {
            // TODO: diagnose and reconnect if necessary...
            println!("Triggering reconnection for `{}`...", addr2);

            // TODO: different way to reconnect ?
            // TODO: unwrap?
            peers.lock().unwrap().remove(&addr2);

            Ok(())
        })
}

// TODO: personnal PID (possibly public key)
// TODO: subfunctions
pub fn swim(local_addr: SocketAddr) -> Result<(), Error> {
    let reader = File::open("./peers.ron")?;
    let addrs: Vec<String> = ron::de::from_reader(reader).unwrap();

    // TODO: beware of deadlocking a peer ?
    let peers = HashMap::new();
    let peers: PeersMap = Arc::new(Mutex::new(peers));

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
            let addr = addr.clone();

            let peer_future =
                Interval::new(Instant::now(), Duration::from_secs(CONNECTION_INTERVAL))
                    .map_err(|err| Error::from(err))
                    .and_then(move |_| -> Box<Future<Item = (), Error = Error> + Send> {
                        let is_already_connected = {
                            // TODO: unwrap?
                            // TODO: async lock?
                            match peers.lock().unwrap().get(&addr) {
                                None => None,
                                Some(socket) => Some(socket.try_clone().unwrap()),
                            }
                        };

                        if let Some(socket) = is_already_connected {
                            // TODO: For health check and reconnection trial (potential collision with normal messages) ?
                            // TODO: can also use some "last time a message was sent..." to test if necessary, ...
                            // TODO: If a ping is pending, prevent sending another one ?
                            let future = ping_peer(peers.clone(), socket, addr);
                            Box::new(future)
                        } else {
                            let future = connect_to_peer(peers.clone(), addr);
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
