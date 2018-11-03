use tokio::codec::Framed;
use tokio::net::TcpStream;
use tokio::prelude::*;
use tokio::runtime::Runtime;

use balthmessage::Message;

use std::fs::File;
use std::net::SocketAddr;

use super::*;

pub mod message_codec;
type MessageCodec = message_codec::MessageCodec;

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
                println!("{:?}", err);
                None
            }
        })
        .filter(|addr| *addr != local_addr)
        .for_each(|addr| {
            let peer_future = TcpStream::connect(&addr)
                .and_then(move |socket| {
                    println!("Connected to : `{}`", addr);

                    let framed_sock = Framed::new(socket, MessageCodec::new());

                    let pid = 0;

                    let manager = framed_sock.take(1).for_each(|msg| {
                        println!("{} : received a message !", pid);
                        Ok(())
                    });

                    task::spawn(manager);

                    // TODO: maybe send `Peer { socket, pid, addr }` to another thread...
                    Ok(())
                })
                .map_err(|err| eprintln!("Could not connect : {:?}", err));

            runtime.spawn(peer_future);
        });
    /*.for_each(|peer_future| {
        runtime.spawn(peer_future);
    });*/

    runtime
        .shutdown_on_idle()
        .wait()
        .map_err(|_| Error::TokioRuntimeError)
}
